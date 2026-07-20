import { fireEvent, render, screen } from "@testing-library/react";
import { beforeEach, describe, expect, it, vi } from "vitest";
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
			pendingSelection: null,
			tasks: [],
		});
		useTransferActivityStore.setState({ expandedActivity: null });
		useFrontendConfigStore.setState({
			archiveDownloadUserEnabled: true,
			isLoaded: true,
		});
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
});
