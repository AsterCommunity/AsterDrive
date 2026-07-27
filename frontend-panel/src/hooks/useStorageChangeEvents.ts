import { useEffect } from "react";
import { config } from "@/config/app";
import { joinApiUrl } from "@/lib/apiUrl";
import {
	invalidateAllFileResourceCaches,
	invalidateFileResourceCachesForMutation,
} from "@/lib/fileResourceCacheInvalidation";
import { logger } from "@/lib/logger";
import { publishStorageChange } from "@/lib/storageChangeBus";
import {
	consumeStorageEventEcho,
	type StorageChangeEventPayload,
} from "@/lib/storageEventEcho";
import {
	decideRemoteStorageMutation,
	type StorageUsageRefreshDecision,
} from "@/lib/storageMutationCoordinator";
import {
	deferStorageRefresh,
	isStorageRefreshGateActive,
} from "@/lib/storageRefreshGate";
import { fileService } from "@/services/fileService";
import { useAuthStore } from "@/stores/authStore";
import { useFileStore } from "@/stores/fileStore";
import { useTeamStore } from "@/stores/teamStore";
import { useWorkspaceStore } from "@/stores/workspaceStore";

/** SSE 应用层重连：指数退避，避免服务端 5xx/401 时浏览器默认 ~3s 间隔的快速重连风暴。 */
const SSE_RECONNECT_BASE_MS = 1_000;
const SSE_RECONNECT_MAX_MS = 30_000;
/** 连续失败次数到阈值后熔断，要求用户刷新或重新登录；防止永久打 backend。 */
const SSE_RECONNECT_FAILURE_LIMIT = 8;
const SSE_INITIAL_CONNECT_DELAY_MS = 1_500;

function currentPathname() {
	if (typeof window === "undefined") {
		return "";
	}
	return window.location.pathname;
}

async function refreshCurrentFolder() {
	const { currentFolderId, navigateTo } = useFileStore.getState();
	try {
		await navigateTo(currentFolderId);
	} catch {
		await navigateTo(null);
	}
}

function invalidatePreviewCaches(fileIds: number[]) {
	for (const fileId of fileIds) {
		invalidateFileResourceCachesForMutation({
			download: fileService.downloadPath(fileId),
			thumbnail: fileService.thumbnailPath(fileId),
			imagePreview: fileService.imagePreviewPath(fileId),
		});
	}
}

function reloadTeamsForCurrentUser() {
	const { user } = useAuthStore.getState();
	void useTeamStore
		.getState()
		.reload(user?.id ?? null)
		.catch(() => undefined);
}

function refreshStorageUsage(decision: StorageUsageRefreshDecision) {
	const { refreshUser } = useAuthStore.getState();
	switch (decision) {
		case "none":
			return;
		case "personal_quota":
			void refreshUser({ fields: ["quota"] });
			return;
		case "teams":
			reloadTeamsForCurrentUser();
			return;
		case "all":
			void refreshUser();
			reloadTeamsForCurrentUser();
			return;
		default: {
			const exhaustiveCheck: never = decision;
			return exhaustiveCheck;
		}
	}
}

export function useStorageChangeEvents() {
	const isAuthenticated = useAuthStore((state) => state.isAuthenticated);
	const isChecking = useAuthStore((state) => state.isChecking);
	const storageEventStreamEnabled = useAuthStore(
		(state) => state.user?.preferences?.storage_event_stream_enabled !== false,
	);

	useEffect(() => {
		if (
			!isAuthenticated ||
			isChecking ||
			!storageEventStreamEnabled ||
			typeof EventSource === "undefined"
		) {
			return;
		}

		// 应用层重连状态：浏览器内置重连只能等固定 3s，且无法感知"该停下来"的情况
		// （比如服务器持续 5xx）。我们手动管理 EventSource 生命周期：
		// 失败 → close → 退避 setTimeout → new EventSource，连续失败超阈值后停止。
		let source: EventSource | null = null;
		let reconnectTimer: ReturnType<typeof setTimeout> | null = null;
		let failureCount = 0;
		let refreshAttemptedForFailureStreak = false;
		let cancelled = false;

		const handleStorageChange = (event: StorageChangeEventPayload) => {
			if (consumeStorageEventEcho(event)) {
				return;
			}

			const workspace = useWorkspaceStore.getState().workspace;
			const { breadcrumb, currentFolderId } = useFileStore.getState();
			const decision = decideRemoteStorageMutation(event, {
				currentWorkspace: workspace,
				folder: {
					currentFolderId,
					breadcrumbFolderIds: breadcrumb.flatMap((item) =>
						item.id === null ? [] : [item.id],
					),
				},
				isRefreshGateActive: isStorageRefreshGateActive(),
				pathname: currentPathname(),
			});

			refreshStorageUsage(decision.refreshStorageUsage);

			if (!decision.publishToWorkspace) {
				return;
			}

			publishStorageChange(event);

			if (decision.invalidateAllResourceCaches) {
				invalidateAllFileResourceCaches();
			} else {
				invalidatePreviewCaches(decision.invalidateFileResourceCacheIds);
			}

			switch (decision.refreshCurrentFolder) {
				case "defer":
					deferStorageRefresh();
					return;
				case "now":
					void refreshCurrentFolder();
					return;
				case "none":
					return;
			}
		};

		const scheduleReconnect = () => {
			if (cancelled || !useAuthStore.getState().isAuthenticated) return;
			if (failureCount >= SSE_RECONNECT_FAILURE_LIMIT) {
				logger.warn(
					`storage event stream gave up after ${failureCount} consecutive failures; reload to retry`,
				);
				return;
			}
			// 指数退避：第 1 次失败后 1s，第 2 次 2s, 4s, ..., 上限 30s。
			// failureCount 在 onerror 里 +1，所以这里指数用 (failureCount - 1)。
			const delay = Math.min(
				SSE_RECONNECT_BASE_MS * 2 ** Math.max(0, failureCount - 1),
				SSE_RECONNECT_MAX_MS,
			);
			logger.debug(
				`scheduling SSE reconnect in ${delay}ms (failure #${failureCount})`,
			);
			reconnectTimer = setTimeout(() => {
				reconnectTimer = null;
				connect();
			}, delay);
		};

		const refreshSessionBeforeReconnect = () => {
			if (
				refreshAttemptedForFailureStreak ||
				!useAuthStore.getState().isAuthenticated
			) {
				scheduleReconnect();
				return;
			}

			refreshAttemptedForFailureStreak = true;
			void useAuthStore
				.getState()
				.refreshToken()
				.catch((error) => {
					logger.debug(
						"storage change event stream auth refresh before reconnect failed",
						error,
					);
				})
				.finally(scheduleReconnect);
		};

		const connect = () => {
			if (cancelled) return;
			source = new EventSource(
				joinApiUrl(config.apiBaseUrl, "/auth/events/storage"),
				{ withCredentials: true },
			);

			source.onopen = () => {
				if (cancelled) return;
				const recovered = failureCount > 0;
				// 连接确认建立后才重置失败计数；浏览器在 onopen 之前的重连不算成功
				failureCount = 0;
				refreshAttemptedForFailureStreak = false;
				if (recovered) {
					handleStorageChange({
						kind: "sync.required",
						workspace: null,
						file_ids: [],
						folder_ids: [],
						affected_parent_ids: [],
						root_affected: false,
						affects_quota: true,
						storage_delta: null,
						at: new Date().toISOString(),
					});
				}
			};

			source.onmessage = (message) => {
				let event: StorageChangeEventPayload;
				try {
					event = JSON.parse(message.data) as StorageChangeEventPayload;
				} catch (error) {
					logger.warn("failed to parse storage change event", error);
					return;
				}

				handleStorageChange(event);
			};

			source.onerror = (error) => {
				// EventSource 的 onerror 不带 HTTP 状态码，无法区分 401/5xx/网络断开。
				// 统一关闭当前连接、走退避重连，不让浏览器默认 3s 重连风暴。
				logger.debug("storage change event stream error", error);
				if (source?.readyState === EventSource.CLOSED) {
					source = null;
					return;
				}
				failureCount += 1;
				source?.close();
				source = null;
				refreshSessionBeforeReconnect();
			};
		};

		reconnectTimer = setTimeout(() => {
			reconnectTimer = null;
			connect();
		}, SSE_INITIAL_CONNECT_DELAY_MS);

		return () => {
			cancelled = true;
			if (reconnectTimer !== null) {
				clearTimeout(reconnectTimer);
				reconnectTimer = null;
			}
			source?.close();
			source = null;
		};
	}, [isAuthenticated, isChecking, storageEventStreamEnabled]);
}

export function StorageChangeEventsBridge() {
	useStorageChangeEvents();
	return null;
}
