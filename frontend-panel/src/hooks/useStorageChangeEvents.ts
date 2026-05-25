import { useEffect } from "react";
import { config } from "@/config/app";
import { invalidateBlobUrl } from "@/hooks/useBlobUrl";
import { invalidateTextContent } from "@/hooks/useTextContent";
import { joinApiUrl } from "@/lib/apiUrl";
import { logger } from "@/lib/logger";
import { publishStorageChange } from "@/lib/storageChangeBus";
import {
	consumeStorageEventEcho,
	type StorageChangeEventPayload,
} from "@/lib/storageEventEcho";
import {
	deferStorageRefresh,
	isStorageRefreshGateActive,
} from "@/lib/storageRefreshGate";
import type { Workspace } from "@/lib/workspace";
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

function eventMatchesWorkspace(
	eventWorkspace: StorageChangeEventPayload["workspace"],
	workspace: Workspace,
) {
	if (!eventWorkspace) {
		return true;
	}
	if (workspace.kind === "personal") {
		return eventWorkspace.kind === "personal";
	}
	return (
		eventWorkspace.kind === "team" &&
		eventWorkspace.team_id === workspace.teamId
	);
}

function shouldRefreshCurrentFolder(event: StorageChangeEventPayload) {
	const { currentFolderId, breadcrumb, searchQuery } = useFileStore.getState();
	if (searchQuery) {
		return false;
	}
	if (event.root_affected && currentFolderId === null) {
		return true;
	}
	if (
		currentFolderId !== null &&
		(event.affected_parent_ids.includes(currentFolderId) ||
			event.folder_ids.includes(currentFolderId))
	) {
		return true;
	}
	return breadcrumb.some(
		(item) => item.id !== null && event.folder_ids.includes(item.id),
	);
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
		invalidateTextContent(fileService.downloadPath(fileId));
		invalidateBlobUrl(fileService.downloadPath(fileId));
		invalidateBlobUrl(fileService.thumbnailPath(fileId));
		invalidateBlobUrl(fileService.imagePreviewPath(fileId));
	}
}

function reloadTeamsForCurrentUser() {
	const { user } = useAuthStore.getState();
	void useTeamStore
		.getState()
		.reload(user?.id ?? null)
		.catch(() => undefined);
}

function eventMayChangeStorageUsage(event: StorageChangeEventPayload) {
	return event.affects_quota;
}

function refreshStorageUsage(event: StorageChangeEventPayload) {
	if (!eventMayChangeStorageUsage(event)) {
		return;
	}

	const { refreshUser } = useAuthStore.getState();
	if (event.kind === "sync.required" || !event.workspace) {
		void refreshUser();
		reloadTeamsForCurrentUser();
		return;
	}
	if (event.workspace.kind === "personal") {
		void refreshUser({ fields: ["quota"] });
		return;
	}
	reloadTeamsForCurrentUser();
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
				// 连接确认建立后才重置失败计数；浏览器在 onopen 之前的重连不算成功
				failureCount = 0;
				refreshAttemptedForFailureStreak = false;
			};

			source.onmessage = (message) => {
				let event: StorageChangeEventPayload;
				try {
					event = JSON.parse(message.data) as StorageChangeEventPayload;
				} catch (error) {
					logger.warn("failed to parse storage change event", error);
					return;
				}

				if (consumeStorageEventEcho(event)) {
					return;
				}

				refreshStorageUsage(event);

				const workspace = useWorkspaceStore.getState().workspace;
				if (!eventMatchesWorkspace(event.workspace, workspace)) {
					return;
				}

				publishStorageChange(event);

				if (event.kind === "sync.required") {
					invalidateBlobUrl();
					invalidateTextContent();
					if (!useFileStore.getState().searchQuery) {
						if (isStorageRefreshGateActive()) {
							deferStorageRefresh();
							return;
						}
						void refreshCurrentFolder();
					}
					return;
				}

				invalidatePreviewCaches(event.file_ids);
				if (shouldRefreshCurrentFolder(event)) {
					if (isStorageRefreshGateActive()) {
						deferStorageRefresh();
						return;
					}
					void refreshCurrentFolder();
				}
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

		connect();

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
