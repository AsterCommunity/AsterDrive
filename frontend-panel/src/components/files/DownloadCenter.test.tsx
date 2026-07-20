import { fireEvent, render, screen, waitFor } from "@testing-library/react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { DownloadCenter } from "@/components/files/DownloadCenter";
import {
	DOWNLOAD_TASK_STATUS,
	type DownloadTask,
	useDownloadStore,
} from "@/stores/downloadStore";
import { useFrontendConfigStore } from "@/stores/frontendConfigStore";
import {
	TRANSFER_ACTIVITY,
	useTransferActivityStore,
} from "@/stores/transferActivityStore";

const mocks = vi.hoisted(() => ({
	cancelDownloadTask: vi.fn(),
	handleApiError: vi.fn(),
	retryDownloadTask: vi.fn(),
	startAuthenticatedDownload: vi.fn(),
	startDirectoryDownload: vi.fn(),
	startProxyArchiveDownload: vi.fn(),
	startProxyFileDownload: vi.fn(),
	streamArchiveDownload: vi.fn(),
	supportsDirectoryDownload: vi.fn(),
	downloadPath: vi.fn(),
}));

vi.mock("@/hooks/useApiError", () => ({
	handleApiError: mocks.handleApiError,
}));

vi.mock("@/services/batchService", () => ({
	createBatchService: () => ({
		streamArchiveDownload: mocks.streamArchiveDownload,
	}),
}));

vi.mock("@/services/downloadCoordinator", () => ({
	cancelDownloadTask: mocks.cancelDownloadTask,
	retryDownloadTask: mocks.retryDownloadTask,
	startDirectoryDownload: mocks.startDirectoryDownload,
	startProxyArchiveDownload: mocks.startProxyArchiveDownload,
	startProxyFileDownload: mocks.startProxyFileDownload,
	supportsDirectoryDownload: mocks.supportsDirectoryDownload,
}));

vi.mock("@/lib/authenticatedDownload", () => ({
	startAuthenticatedDownload: mocks.startAuthenticatedDownload,
}));

vi.mock("@/services/fileService", () => ({
	createFileService: () => ({ downloadPath: mocks.downloadPath }),
}));

vi.mock("react-i18next", async (importOriginal) => {
	const actual = await importOriginal<typeof import("react-i18next")>();
	return {
		...actual,
		useTranslation: () => ({
			t: (key: string, options?: Record<string, unknown>) => {
				const translations: Record<string, string> = {
					download_center: "下载中心",
					download_center_completed: "下载完成",
					download_center_completed_summary: `已完成 ${options?.count} 项下载`,
					download_center_failed_summary: `已完成 ${options?.completed} 项，失败 ${options?.failed} 项`,
					download_center_progress_summary: `已完成 ${options?.completed} / ${options?.total} 项`,
				};
				return translations[key] ?? key;
			},
		}),
	};
});

function task(overrides: Partial<DownloadTask> = {}): DownloadTask {
	return {
		id: "download-1",
		kind: "directory",
		name: "download_to_folder",
		status: DOWNLOAD_TASK_STATUS.downloading,
		createdAt: 1,
		bytesReceived: 50,
		totalBytes: 100,
		speedBps: 10,
		completedItems: 2,
		failedItems: 0,
		totalItems: 4,
		items: [],
		...overrides,
	};
}

describe("DownloadCenter", () => {
	beforeEach(() => {
		mocks.cancelDownloadTask.mockReset();
		mocks.handleApiError.mockReset();
		mocks.retryDownloadTask.mockReset();
		mocks.startAuthenticatedDownload.mockReset();
		mocks.startDirectoryDownload.mockReset();
		mocks.startProxyArchiveDownload.mockReset();
		mocks.startProxyFileDownload.mockReset();
		mocks.streamArchiveDownload.mockReset();
		mocks.streamArchiveDownload.mockResolvedValue(undefined);
		mocks.supportsDirectoryDownload.mockReset();
		mocks.supportsDirectoryDownload.mockReturnValue(false);
		mocks.downloadPath.mockReset();
		mocks.downloadPath.mockReturnValue("/files/1/download");
		useDownloadStore.setState({
			pendingSelection: null,
			tasks: [],
		});
		useTransferActivityStore.setState({ expandedActivity: null });
		useFrontendConfigStore.setState({
			archiveDownloadUserEnabled: true,
			isLoaded: true,
		});
	});

	afterEach(() => {
		vi.clearAllMocks();
	});

	it("shows a full-width active download section with progress", () => {
		useDownloadStore.setState({ tasks: [task()] });

		render(<DownloadCenter />);

		const trigger = screen.getByRole("button", { name: "下载中心" });
		expect(trigger.closest("section")).toHaveClass(
			"pointer-events-auto",
			"w-full",
		);
		expect(trigger).toHaveAttribute("aria-expanded", "false");
		expect(trigger).toHaveTextContent("50%");
		expect(trigger).toHaveTextContent("已完成 2 / 4 项");
	});

	it("shows the completed state", () => {
		useDownloadStore.setState({
			tasks: [
				task({
					status: DOWNLOAD_TASK_STATUS.completed,
					bytesReceived: 100,
					completedItems: 4,
				}),
			],
		});

		render(<DownloadCenter />);

		const trigger = screen.getByRole("button", { name: "下载中心" });
		expect(trigger).toHaveTextContent("下载完成");
		expect(trigger).toHaveTextContent("已完成 4 项下载");
		expect(trigger).not.toHaveTextContent("100%");
	});

	it("does not expand task details when a task is added", () => {
		useDownloadStore.getState().upsertTask(task());

		render(<DownloadCenter />);

		expect(useTransferActivityStore.getState().expandedActivity).toBeNull();
		expect(screen.getByRole("button", { name: "下载中心" })).toHaveAttribute(
			"aria-expanded",
			"false",
		);
	});

	it("expands task details inline in the bottom-right activity shell", () => {
		useDownloadStore.setState({ tasks: [task()] });

		render(<DownloadCenter />);
		fireEvent.click(screen.getByRole("button", { name: "下载中心" }));

		expect(useTransferActivityStore.getState().expandedActivity).toBe(
			TRANSFER_ACTIVITY.download,
		);
		expect(screen.getByText(/download_status_downloading/)).toBeInTheDocument();
	});

	it("offers browser-managed ZIP downloads for multi-selection", async () => {
		useDownloadStore.setState({
			pendingSelection: {
				workspace: { kind: "team", teamId: 9 },
				files: [
					{ id: 1, name: "first.txt" },
					{ id: 2, name: "second.txt" },
				],
				folders: [{ id: 3, name: "docs" }],
			},
		});

		render(<DownloadCenter />);
		fireEvent.click(
			screen.getByRole("button", { name: /download_browser_archive/ }),
		);

		expect(mocks.streamArchiveDownload).toHaveBeenCalledWith([1, 2], [3]);
		expect(useDownloadStore.getState().pendingSelection).toBeNull();
	});

	it("hides ZIP download methods when archive downloads are disabled", () => {
		useFrontendConfigStore.setState({
			archiveDownloadUserEnabled: false,
			isLoaded: true,
		});
		useDownloadStore.setState({
			pendingSelection: {
				workspace: { kind: "personal" },
				files: [
					{ id: 1, name: "first.txt" },
					{ id: 2, name: "second.txt" },
				],
				folders: [{ id: 3, name: "docs" }],
			},
		});

		render(<DownloadCenter />);

		expect(
			screen.queryByRole("button", { name: /download_proxy_archive/ }),
		).not.toBeInTheDocument();
		expect(
			screen.queryByRole("button", { name: /download_browser_archive/ }),
		).not.toBeInTheDocument();
		expect(
			screen.queryByRole("button", { name: /download_to_folder/ }),
		).not.toBeInTheDocument();
	});

	it("keeps native directory download available when ZIP archives are disabled", () => {
		mocks.supportsDirectoryDownload.mockReturnValue(true);
		useFrontendConfigStore.setState({
			archiveDownloadUserEnabled: false,
			isLoaded: true,
		});
		const pendingSelection = {
			workspace: { kind: "personal" as const },
			files: [],
			folders: [{ id: 3, name: "docs" }],
		};
		useDownloadStore.setState({ pendingSelection });

		render(<DownloadCenter />);
		fireEvent.click(screen.getByRole("button", { name: /download_to_folder/ }));

		expect(mocks.startDirectoryDownload).toHaveBeenCalledWith(pendingSelection);
		expect(mocks.startProxyArchiveDownload).not.toHaveBeenCalled();
		expect(useDownloadStore.getState().pendingSelection).toBeNull();
	});

	it("falls back from directory download to proxy ZIP only when archive capability is enabled", () => {
		const pendingSelection = {
			workspace: { kind: "personal" as const },
			files: [],
			folders: [{ id: 3, name: "docs" }],
		};
		useDownloadStore.setState({ pendingSelection });

		render(<DownloadCenter />);
		expect(screen.getByText("download_directory_fallback")).toBeInTheDocument();
		fireEvent.click(screen.getByRole("button", { name: /download_to_folder/ }));

		expect(mocks.startProxyArchiveDownload).toHaveBeenCalledWith(
			pendingSelection,
		);
		expect(mocks.startDirectoryDownload).not.toHaveBeenCalled();
	});

	it("keeps both single-file methods when archive capability is disabled", () => {
		useFrontendConfigStore.setState({
			archiveDownloadUserEnabled: false,
			isLoaded: true,
		});
		const pendingSelection = {
			workspace: { kind: "team" as const, teamId: 9 },
			files: [{ id: 1, name: "report.txt", size: 4 }],
			folders: [],
		};
		useDownloadStore.setState({ pendingSelection });

		const { rerender } = render(<DownloadCenter />);
		fireEvent.click(
			screen.getByRole("button", { name: /download_proxy_file/ }),
		);
		expect(mocks.startProxyFileDownload).toHaveBeenCalledWith(
			pendingSelection.workspace,
			pendingSelection.files[0],
		);

		useDownloadStore.setState({ pendingSelection });
		rerender(<DownloadCenter />);
		fireEvent.click(
			screen.getByRole("button", { name: /download_browser_default/ }),
		);
		expect(mocks.downloadPath).toHaveBeenCalledWith(1);
		expect(mocks.startAuthenticatedDownload).toHaveBeenCalledWith(
			"/files/1/download",
		);
	});

	it("dismisses the method dialog through its open-state callback", () => {
		useDownloadStore.setState({
			pendingSelection: {
				workspace: { kind: "personal" },
				files: [{ id: 1, name: "report.txt" }],
				folders: [],
			},
		});

		render(<DownloadCenter />);
		fireEvent.keyDown(document, { key: "Escape" });

		expect(useDownloadStore.getState().pendingSelection).toBeNull();
	});

	it("renders cancel and retry actions for active and failed tasks", () => {
		useDownloadStore.setState({
			tasks: [
				task({ id: "active", status: DOWNLOAD_TASK_STATUS.downloading }),
				task({
					id: "failed",
					status: DOWNLOAD_TASK_STATUS.failed,
					failedItems: 1,
					error: "download_items_failed",
				}),
			],
		});
		useTransferActivityStore.setState({
			expandedActivity: TRANSFER_ACTIVITY.download,
		});

		render(<DownloadCenter />);
		fireEvent.click(screen.getByRole("button", { name: "download_cancel" }));
		fireEvent.click(
			screen.getByRole("button", { name: "download_retry_failed" }),
		);

		expect(mocks.cancelDownloadTask).toHaveBeenCalledWith("active");
		expect(mocks.retryDownloadTask).toHaveBeenCalledWith("failed");
	});

	it("clears terminal tasks, preserves failed work, and closes an empty section", async () => {
		useDownloadStore.setState({
			tasks: [
				task({ id: "completed", status: DOWNLOAD_TASK_STATUS.completed }),
				task({ id: "canceled", status: DOWNLOAD_TASK_STATUS.canceled }),
			],
		});
		useTransferActivityStore.setState({
			expandedActivity: TRANSFER_ACTIVITY.download,
		});

		render(<DownloadCenter />);
		fireEvent.click(
			screen.getByRole("button", { name: /download_clear_completed/ }),
		);

		await waitFor(() => {
			expect(screen.queryByRole("button", { name: "下载中心" })).toBeNull();
			expect(useTransferActivityStore.getState().expandedActivity).toBeNull();
		});
	});

	it("keeps a failed task when clearing completed downloads", () => {
		useDownloadStore.setState({
			tasks: [
				task({ id: "completed", status: DOWNLOAD_TASK_STATUS.completed }),
				task({ id: "failed", status: DOWNLOAD_TASK_STATUS.failed }),
			],
		});
		useTransferActivityStore.setState({
			expandedActivity: TRANSFER_ACTIVITY.download,
		});

		render(<DownloadCenter />);
		fireEvent.click(
			screen.getByRole("button", { name: /download_clear_completed/ }),
		);

		expect(useDownloadStore.getState().tasks.map(({ id }) => id)).toEqual([
			"failed",
		]);
	});
});
