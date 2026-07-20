import { fireEvent, render, screen } from "@testing-library/react";
import { beforeEach, describe, expect, it, vi } from "vitest";
import { DownloadCenter } from "@/components/files/DownloadCenter";
import {
	DOWNLOAD_TASK_STATUS,
	type DownloadTask,
	useDownloadStore,
} from "@/stores/downloadStore";

const mocks = vi.hoisted(() => ({
	handleApiError: vi.fn(),
	streamArchiveDownload: vi.fn(),
}));

vi.mock("@/hooks/useApiError", () => ({
	handleApiError: mocks.handleApiError,
}));

vi.mock("@/services/batchService", () => ({
	createBatchService: () => ({
		streamArchiveDownload: mocks.streamArchiveDownload,
	}),
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
		mocks.handleApiError.mockReset();
		mocks.streamArchiveDownload.mockReset();
		mocks.streamArchiveDownload.mockResolvedValue(undefined);
		useDownloadStore.setState({
			isPanelOpen: false,
			pendingSelection: null,
			tasks: [],
		});
	});

	it("shows an active blue bordered status bar with progress and completed items", () => {
		useDownloadStore.setState({ tasks: [task()] });

		render(<DownloadCenter />);

		const trigger = screen.getByRole("button", { name: "下载中心" });
		expect(trigger).toHaveClass("border-blue-500/70", "pointer-events-auto");
		expect(trigger).not.toHaveClass("fixed");
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
		expect(trigger).toHaveClass("border-emerald-500/55");
		expect(trigger).toHaveTextContent("下载完成");
		expect(trigger).toHaveTextContent("已完成 4 项下载");
		expect(trigger).toHaveTextContent("100%");
	});

	it("does not open the details dialog when a task is added", () => {
		useDownloadStore.getState().upsertTask(task());

		render(<DownloadCenter />);

		expect(useDownloadStore.getState().isPanelOpen).toBe(false);
		expect(screen.queryByText("download_center_desc")).not.toBeInTheDocument();
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
});
