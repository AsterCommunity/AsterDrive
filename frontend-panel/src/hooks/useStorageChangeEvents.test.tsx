import { renderHook, waitFor } from "@testing-library/react";
import { beforeEach, describe, expect, it, vi } from "vitest";
import {
	clearStorageEventEchoes,
	rememberStorageEventEcho,
} from "@/lib/storageEventEcho";

const mockState = vi.hoisted(() => ({
	auth: {
		isAuthenticated: true,
		isChecking: false,
		refreshToken: vi.fn(),
		refreshUser: vi.fn(),
		user: {
			id: 100,
			preferences: {
				storage_event_stream_enabled: true,
			},
		},
	},
	teamStore: {
		reload: vi.fn(),
	},
	workspace: { kind: "personal" } as
		| { kind: "personal" }
		| { kind: "team"; teamId: number },
	fileStore: {
		currentFolderId: 7,
		breadcrumb: [
			{ id: null, name: "Root" },
			{ id: 7, name: "Docs" },
		],
		searchQuery: null as string | null,
		navigateTo: vi.fn(),
	},
	invalidateBlobUrl: vi.fn(),
	invalidateTextContent: vi.fn(),
	storageRefreshGate: {
		deferStorageRefresh: vi.fn(),
		isStorageRefreshGateActive: vi.fn(() => false),
	},
}));

class MockEventSource {
	static instances: MockEventSource[] = [];
	static CONNECTING = 0;
	static OPEN = 1;
	static CLOSED = 2;

	onerror: ((event: Event) => void) | null = null;
	onmessage: ((event: MessageEvent<string>) => void) | null = null;
	onopen: ((event: Event) => void) | null = null;
	close = vi.fn();
	readyState = MockEventSource.CONNECTING;
	url: string;
	withCredentials: boolean;

	constructor(url: string, init?: EventSourceInit) {
		this.url = url;
		this.withCredentials = init?.withCredentials ?? false;
		MockEventSource.instances.push(this);
	}

	emit(data: unknown) {
		this.onmessage?.({ data: JSON.stringify(data) } as MessageEvent<string>);
	}

	triggerError() {
		this.onerror?.(new Event("error"));
	}

	triggerOpen() {
		this.readyState = MockEventSource.OPEN;
		this.onopen?.(new Event("open"));
	}

	triggerClosedError() {
		this.readyState = MockEventSource.CLOSED;
		this.onerror?.(new Event("error"));
	}

	static reset() {
		MockEventSource.instances = [];
	}
}

async function connectStorageEvents() {
	await waitFor(
		() => {
			expect(MockEventSource.instances).toHaveLength(1);
		},
		{ timeout: 2_000 },
	);
}

vi.mock("@/config/app", () => ({
	config: {
		apiBaseUrl: "http://api.test/api/v1",
	},
}));

vi.mock("@/hooks/useBlobUrl", () => ({
	invalidateBlobUrl: (...args: unknown[]) =>
		mockState.invalidateBlobUrl(...args),
}));

vi.mock("@/hooks/useTextContent", () => ({
	invalidateTextContent: (...args: unknown[]) =>
		mockState.invalidateTextContent(...args),
}));

vi.mock("@/lib/storageRefreshGate", () => ({
	deferStorageRefresh: (...args: unknown[]) =>
		mockState.storageRefreshGate.deferStorageRefresh(...args),
	isStorageRefreshGateActive: (...args: unknown[]) =>
		mockState.storageRefreshGate.isStorageRefreshGateActive(...args),
}));

vi.mock("@/services/fileService", () => ({
	fileService: {
		downloadPath: (id: number) => `/files/${id}/download`,
		thumbnailPath: (id: number) => `/files/${id}/thumbnail`,
		imagePreviewPath: (id: number) => `/files/${id}/image-preview`,
	},
}));

vi.mock("@/stores/authStore", () => {
	const useAuthStore = Object.assign(
		<T,>(selector: (state: typeof mockState.auth) => T) =>
			selector(mockState.auth),
		{
			getState: () => mockState.auth,
		},
	);

	return { useAuthStore };
});

vi.mock("@/stores/teamStore", () => ({
	useTeamStore: {
		getState: () => mockState.teamStore,
	},
}));

vi.mock("@/stores/workspaceStore", () => {
	const useWorkspaceStore = Object.assign(
		<T,>(selector: (state: { workspace: typeof mockState.workspace }) => T) =>
			selector({ workspace: mockState.workspace }),
		{
			getState: () => ({ workspace: mockState.workspace }),
		},
	);

	return { useWorkspaceStore };
});

vi.mock("@/stores/fileStore", () => {
	const useFileStore = Object.assign(
		<T,>(
			selector: (state: {
				breadcrumb: typeof mockState.fileStore.breadcrumb;
				currentFolderId: number | null;
				navigateTo: typeof mockState.fileStore.navigateTo;
				searchQuery: string | null;
			}) => T,
		) => selector(mockState.fileStore),
		{
			getState: () => mockState.fileStore,
		},
	);

	return { useFileStore };
});

describe("useStorageChangeEvents", () => {
	beforeEach(() => {
		MockEventSource.reset();
		mockState.auth.isAuthenticated = true;
		mockState.auth.isChecking = false;
		mockState.auth.refreshToken.mockReset();
		mockState.auth.refreshToken.mockResolvedValue(undefined);
		mockState.auth.refreshUser.mockReset();
		mockState.auth.refreshUser.mockResolvedValue(undefined);
		mockState.auth.user.preferences.storage_event_stream_enabled = true;
		mockState.teamStore.reload.mockReset();
		mockState.teamStore.reload.mockResolvedValue(undefined);
		mockState.workspace = { kind: "personal" };
		mockState.fileStore.currentFolderId = 7;
		mockState.fileStore.breadcrumb = [
			{ id: null, name: "Root" },
			{ id: 7, name: "Docs" },
		];
		mockState.fileStore.searchQuery = null;
		mockState.fileStore.navigateTo.mockReset();
		mockState.fileStore.navigateTo.mockResolvedValue(undefined);
		mockState.invalidateBlobUrl.mockReset();
		mockState.invalidateTextContent.mockReset();
		mockState.storageRefreshGate.deferStorageRefresh.mockReset();
		mockState.storageRefreshGate.isStorageRefreshGateActive.mockReset();
		mockState.storageRefreshGate.isStorageRefreshGateActive.mockReturnValue(
			false,
		);
		clearStorageEventEchoes();
		vi.stubGlobal("EventSource", MockEventSource);
	});

	it("invalidates matching file previews and refreshes the current folder", async () => {
		const { useStorageChangeEvents } = await import(
			"@/hooks/useStorageChangeEvents"
		);

		const hook = renderHook(() => useStorageChangeEvents());

		await connectStorageEvents();

		MockEventSource.instances[0]?.emit({
			kind: "file.updated",
			workspace: { kind: "personal" },
			file_ids: [11],
			folder_ids: [],
			affected_parent_ids: [7],
			root_affected: false,
			affects_quota: false,
			storage_delta: null,
			at: "2026-04-08T00:00:00Z",
		});

		await waitFor(() => {
			expect(mockState.invalidateTextContent).toHaveBeenCalledWith(
				"/files/11/download",
			);
		});
		expect(mockState.invalidateBlobUrl).toHaveBeenNthCalledWith(
			1,
			"/files/11/download",
		);
		expect(mockState.invalidateBlobUrl).toHaveBeenNthCalledWith(
			2,
			"/files/11/thumbnail",
		);
		await waitFor(() => {
			expect(mockState.fileStore.navigateTo).toHaveBeenCalledWith(7);
		});
		expect(mockState.auth.refreshUser).not.toHaveBeenCalled();
		expect(mockState.teamStore.reload).not.toHaveBeenCalled();

		hook.unmount();
		expect(MockEventSource.instances[0]?.close).toHaveBeenCalledTimes(1);
	});

	it("handles sync.required without refreshing during search", async () => {
		mockState.fileStore.searchQuery = "report";
		const { useStorageChangeEvents } = await import(
			"@/hooks/useStorageChangeEvents"
		);

		renderHook(() => useStorageChangeEvents());

		await connectStorageEvents();

		MockEventSource.instances[0]?.emit({
			kind: "sync.required",
			workspace: null,
			file_ids: [],
			folder_ids: [],
			affected_parent_ids: [],
			root_affected: false,
			affects_quota: true,
			storage_delta: null,
			at: "2026-04-08T00:00:00Z",
		});

		await waitFor(() => {
			expect(mockState.invalidateBlobUrl).toHaveBeenCalledWith();
		});
		expect(mockState.invalidateTextContent).toHaveBeenCalledWith();
		expect(mockState.auth.refreshUser).toHaveBeenCalledTimes(1);
		expect(mockState.teamStore.reload).toHaveBeenCalledWith(100);
		expect(mockState.fileStore.navigateTo).not.toHaveBeenCalled();
	});

	it("ignores non-quota events from other workspaces", async () => {
		mockState.workspace = { kind: "team", teamId: 9 };
		const { useStorageChangeEvents } = await import(
			"@/hooks/useStorageChangeEvents"
		);

		renderHook(() => useStorageChangeEvents());

		await connectStorageEvents();

		MockEventSource.instances[0]?.emit({
			kind: "file.trashed",
			workspace: { kind: "team", team_id: 42 },
			file_ids: [5],
			folder_ids: [],
			affected_parent_ids: [7],
			root_affected: false,
			affects_quota: false,
			storage_delta: null,
			at: "2026-04-08T00:00:00Z",
		});

		expect(mockState.teamStore.reload).not.toHaveBeenCalled();
		expect(mockState.invalidateBlobUrl).not.toHaveBeenCalled();
		expect(mockState.invalidateTextContent).not.toHaveBeenCalled();
		expect(mockState.fileStore.navigateTo).not.toHaveBeenCalled();
	});

	it("reloads teams for quota-affecting team events from other workspaces", async () => {
		mockState.workspace = { kind: "team", teamId: 9 };
		const { useStorageChangeEvents } = await import(
			"@/hooks/useStorageChangeEvents"
		);

		renderHook(() => useStorageChangeEvents());

		await connectStorageEvents();

		MockEventSource.instances[0]?.emit({
			kind: "file.created",
			workspace: { kind: "team", team_id: 42 },
			file_ids: [5],
			folder_ids: [],
			affected_parent_ids: [7],
			root_affected: false,
			affects_quota: true,
			storage_delta: 64,
			at: "2026-04-08T00:00:00Z",
		});

		await waitFor(() => {
			expect(mockState.teamStore.reload).toHaveBeenCalledWith(100);
		});
		expect(mockState.invalidateBlobUrl).not.toHaveBeenCalled();
		expect(mockState.invalidateTextContent).not.toHaveBeenCalled();
		expect(mockState.fileStore.navigateTo).not.toHaveBeenCalled();
	});

	it("refreshes only personal quota for quota-affecting personal events", async () => {
		const { useStorageChangeEvents } = await import(
			"@/hooks/useStorageChangeEvents"
		);

		renderHook(() => useStorageChangeEvents());

		await connectStorageEvents();

		MockEventSource.instances[0]?.emit({
			kind: "file.purged",
			workspace: { kind: "personal" },
			file_ids: [5],
			folder_ids: [],
			affected_parent_ids: [7],
			root_affected: false,
			affects_quota: true,
			storage_delta: -12,
			at: "2026-04-08T00:00:00Z",
		});

		await waitFor(() => {
			expect(mockState.auth.refreshUser).toHaveBeenCalledWith({
				fields: ["quota"],
			});
		});
		expect(mockState.teamStore.reload).not.toHaveBeenCalled();
	});

	it("defers folder refresh while the upload queue gate is active", async () => {
		mockState.storageRefreshGate.isStorageRefreshGateActive.mockReturnValue(
			true,
		);
		const { useStorageChangeEvents } = await import(
			"@/hooks/useStorageChangeEvents"
		);

		renderHook(() => useStorageChangeEvents());

		await connectStorageEvents();

		MockEventSource.instances[0]?.emit({
			kind: "file.updated",
			workspace: { kind: "personal" },
			file_ids: [12],
			folder_ids: [],
			affected_parent_ids: [7],
			root_affected: false,
			affects_quota: false,
			storage_delta: null,
			at: "2026-04-08T00:00:00Z",
		});

		await waitFor(() => {
			expect(mockState.invalidateTextContent).toHaveBeenCalledWith(
				"/files/12/download",
			);
		});
		expect(mockState.storageRefreshGate.deferStorageRefresh).toHaveBeenCalled();
		expect(mockState.fileStore.navigateTo).not.toHaveBeenCalled();
	});

	it("ignores matching local mutation echo events", async () => {
		rememberStorageEventEcho({
			kind: "file.trashed",
			workspace: { kind: "personal" },
			fileIds: [12],
		});
		const { useStorageChangeEvents } = await import(
			"@/hooks/useStorageChangeEvents"
		);

		renderHook(() => useStorageChangeEvents());

		await connectStorageEvents();

		MockEventSource.instances[0]?.emit({
			kind: "file.trashed",
			workspace: { kind: "personal" },
			file_ids: [12],
			folder_ids: [],
			affected_parent_ids: [7],
			root_affected: false,
			affects_quota: false,
			storage_delta: null,
			at: "2026-04-08T00:00:00Z",
		});

		expect(mockState.auth.refreshUser).not.toHaveBeenCalled();
		expect(mockState.invalidateBlobUrl).not.toHaveBeenCalled();
		expect(mockState.invalidateTextContent).not.toHaveBeenCalled();
		expect(mockState.fileStore.navigateTo).not.toHaveBeenCalled();
	});

	it("does not open the event stream when the user disables realtime sync", async () => {
		vi.useFakeTimers();
		mockState.auth.user.preferences.storage_event_stream_enabled = false;
		const { useStorageChangeEvents } = await import(
			"@/hooks/useStorageChangeEvents"
		);

		renderHook(() => useStorageChangeEvents());

		await vi.advanceTimersByTimeAsync(1600);
		expect(MockEventSource.instances).toHaveLength(0);
	});

	it("does not open the event stream while auth bootstrap is checking", async () => {
		vi.useFakeTimers();
		mockState.auth.isChecking = true;
		const { useStorageChangeEvents } = await import(
			"@/hooks/useStorageChangeEvents"
		);

		renderHook(() => useStorageChangeEvents());

		await vi.advanceTimersByTimeAsync(1600);
		expect(MockEventSource.instances).toHaveLength(0);
	});

	it("reconnects with exponential backoff after onerror", async () => {
		vi.useFakeTimers();
		try {
			const { useStorageChangeEvents } = await import(
				"@/hooks/useStorageChangeEvents"
			);
			renderHook(() => useStorageChangeEvents());

			await vi.advanceTimersByTimeAsync(1500);
			expect(MockEventSource.instances).toHaveLength(1);

			// 第 1 次失败 → 退避 1000ms 后重连
			MockEventSource.instances[0]?.triggerError();
			expect(MockEventSource.instances[0]?.close).toHaveBeenCalledTimes(1);
			expect(mockState.auth.refreshToken).toHaveBeenCalledTimes(1);
			expect(MockEventSource.instances).toHaveLength(1);
			await vi.advanceTimersByTimeAsync(999);
			expect(MockEventSource.instances).toHaveLength(1);
			await vi.advanceTimersByTimeAsync(1);
			expect(MockEventSource.instances).toHaveLength(2);

			// 第 2 次失败 → 2000ms 后重连
			MockEventSource.instances[1]?.triggerError();
			await vi.advanceTimersByTimeAsync(2000);
			expect(MockEventSource.instances).toHaveLength(3);

			// 第 3 次失败 → 4000ms
			MockEventSource.instances[2]?.triggerError();
			await vi.advanceTimersByTimeAsync(4000);
			expect(MockEventSource.instances).toHaveLength(4);
		} finally {
			vi.useRealTimers();
		}
	});

	it("refreshes the session before reconnecting after onerror", async () => {
		vi.useFakeTimers();
		try {
			const { useStorageChangeEvents } = await import(
				"@/hooks/useStorageChangeEvents"
			);
			renderHook(() => useStorageChangeEvents());

			await vi.advanceTimersByTimeAsync(1500);
			MockEventSource.instances[0]?.triggerError();

			expect(mockState.auth.refreshToken).toHaveBeenCalledTimes(1);
			expect(MockEventSource.instances).toHaveLength(1);

			await vi.advanceTimersByTimeAsync(1000);

			expect(MockEventSource.instances).toHaveLength(2);
		} finally {
			vi.useRealTimers();
		}
	});

	it("does not reconnect when auth refresh clears the session", async () => {
		vi.useFakeTimers();
		try {
			mockState.auth.refreshToken.mockImplementationOnce(async () => {
				mockState.auth.isAuthenticated = false;
				throw new Error("refresh failed");
			});
			const { useStorageChangeEvents } = await import(
				"@/hooks/useStorageChangeEvents"
			);
			renderHook(() => useStorageChangeEvents());

			await vi.advanceTimersByTimeAsync(1500);
			MockEventSource.instances[0]?.triggerError();
			await vi.advanceTimersByTimeAsync(60_000);

			expect(mockState.auth.refreshToken).toHaveBeenCalledTimes(1);
			expect(MockEventSource.instances).toHaveLength(1);
		} finally {
			vi.useRealTimers();
		}
	});

	it("does not reconnect after the server permanently closes the stream", async () => {
		vi.useFakeTimers();
		try {
			const { useStorageChangeEvents } = await import(
				"@/hooks/useStorageChangeEvents"
			);
			renderHook(() => useStorageChangeEvents());

			await vi.advanceTimersByTimeAsync(1500);
			expect(MockEventSource.instances).toHaveLength(1);

			MockEventSource.instances[0]?.triggerClosedError();
			await vi.advanceTimersByTimeAsync(60_000);

			expect(MockEventSource.instances[0]?.close).not.toHaveBeenCalled();
			expect(MockEventSource.instances).toHaveLength(1);
		} finally {
			vi.useRealTimers();
		}
	});

	it("resets failure count after a successful onopen", async () => {
		vi.useFakeTimers();
		try {
			const { useStorageChangeEvents } = await import(
				"@/hooks/useStorageChangeEvents"
			);
			renderHook(() => useStorageChangeEvents());

			await vi.advanceTimersByTimeAsync(1500);
			// 失败 1 次累计 failureCount=1，等待 1000ms 后重连
			MockEventSource.instances[0]?.triggerError();
			await vi.advanceTimersByTimeAsync(1000);
			expect(MockEventSource.instances).toHaveLength(2);

			// 第 2 个连接成功 onopen → 重置计数
			MockEventSource.instances[1]?.triggerOpen();

			// 再次失败应当回到 1000ms（如果未重置就是 2000ms）
			MockEventSource.instances[1]?.triggerError();
			await vi.advanceTimersByTimeAsync(999);
			expect(MockEventSource.instances).toHaveLength(2);
			await vi.advanceTimersByTimeAsync(1);
			expect(MockEventSource.instances).toHaveLength(3);
		} finally {
			vi.useRealTimers();
		}
	});

	it("stops reconnecting after the failure limit and cleans up on unmount", async () => {
		vi.useFakeTimers();
		try {
			const { useStorageChangeEvents } = await import(
				"@/hooks/useStorageChangeEvents"
			);
			const hook = renderHook(() => useStorageChangeEvents());

			await vi.advanceTimersByTimeAsync(1500);
			// 触发 8 次连续失败（达到 SSE_RECONNECT_FAILURE_LIMIT）
			for (let i = 0; i < 8; i += 1) {
				MockEventSource.instances[i]?.triggerError();
				// 退避上限 30s 足以覆盖最大 delay（2^7 * 1000 = 128000 → cap 30000）
				await vi.advanceTimersByTimeAsync(30_000);
			}
			// 此时应已建 8 个 EventSource（i=0..7 的 instance）
			expect(MockEventSource.instances).toHaveLength(8);

			// 第 8 次失败 → failureCount=8 = limit → 不再 schedule
			MockEventSource.instances[7]?.triggerError();
			await vi.advanceTimersByTimeAsync(60_000);
			expect(MockEventSource.instances).toHaveLength(8);

			// unmount 时清理 timer + 关闭 source（不 throw）
			expect(() => hook.unmount()).not.toThrow();
		} finally {
			vi.useRealTimers();
		}
	});
});
