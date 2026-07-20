import { fireEvent, render, screen } from "@testing-library/react";
import { describe, expect, it, vi } from "vitest";
import {
	UploadPanel,
	type UploadTaskView,
} from "@/components/files/UploadPanel";

vi.mock("@tanstack/react-virtual", () => ({
	useVirtualizer: ({
		count,
		estimateSize,
	}: {
		count: number;
		estimateSize: (index: number) => number;
	}) => {
		let start = 0;
		const items = Array.from({ length: count }, (_, index) => {
			const size = estimateSize(index);
			const item = { index, size, start };
			start += size;
			return item;
		});

		return {
			getTotalSize: () => start,
			getVirtualItems: () => items,
		};
	},
}));

vi.mock("react-i18next", () => ({
	useTranslation: () => ({
		t: (key: string, opts?: Record<string, unknown>) => {
			if (key === "root") return "Root";
			if (key === "upload_group_item_count") {
				return `${opts?.count} item(s)`;
			}
			return key;
		},
	}),
}));

function renderPanel(tasks: UploadTaskView[]) {
	const onClearCompleted = vi.fn();
	const onRetryFailed = vi.fn();
	const onToggle = vi.fn();

	render(
		<UploadPanel
			open
			onToggle={onToggle}
			title="Uploads"
			summary="3 tasks"
			tasks={tasks}
			emptyText="No tasks"
			totalCount={tasks.length}
			successCount={1}
			failedCount={1}
			activeCount={1}
			overallProgress={67}
			onRetryFailed={onRetryFailed}
			retryFailedLabel="Retry failed"
			onClearCompleted={onClearCompleted}
			clearCompletedLabel="Clear completed"
		/>,
	);

	return { onClearCompleted, onRetryFailed, onToggle };
}

describe("UploadPanel", () => {
	it("renders the empty state when there are no tasks", () => {
		render(
			<UploadPanel
				open
				onToggle={vi.fn()}
				title="Uploads"
				summary="0 tasks"
				tasks={[]}
				emptyText="No tasks"
			/>,
		);

		expect(screen.getByText("No tasks")).toBeInTheDocument();
	});

	it("groups tasks and exposes footer actions", () => {
		const tasks: UploadTaskView[] = [
			{
				id: "root-done",
				title: "done.txt",
				status: "Done",
				mode: "Direct",
				progress: 100,
				completed: true,
				targetLabel: "Projects",
			},
			{
				id: "folder-active",
				title: "active.txt",
				status: "Uploading",
				mode: "Chunked",
				progress: 55,
				group: "Folder A",
				targetLabel: "Projects",
			},
			{
				id: "folder-failed",
				title: "failed.txt",
				status: "Failed",
				mode: "Presigned",
				progress: 30,
				group: "Folder B",
				targetLabel: "Projects",
				actions: [
					{
						label: "Retry",
						icon: "ArrowsClockwise",
						onClick: vi.fn(),
					},
				],
			},
		];

		const { onClearCompleted, onRetryFailed, onToggle } = renderPanel(tasks);

		expect(screen.getByText("3 tasks")).toBeInTheDocument();
		expect(screen.getByText("67%")).toBeInTheDocument();

		expect(screen.getByText("Root")).toBeInTheDocument();
		expect(screen.getByText("upload_batch_done")).toBeInTheDocument();
		expect(screen.getByText("Folder A")).toBeInTheDocument();
		expect(screen.getByText("upload_batch_active")).toBeInTheDocument();
		expect(screen.getByText("Folder B")).toBeInTheDocument();
		expect(screen.getByText("upload_batch_partial_failed")).toBeInTheDocument();
		expect(screen.getAllByText("1 item(s)")).toHaveLength(3);

		fireEvent.click(screen.getByText("Retry failed"));
		fireEvent.click(screen.getByText("Clear completed"));
		fireEvent.click(
			screen.getByRole("button", { name: "upload_panel_collapse" }),
		);

		expect(onRetryFailed).toHaveBeenCalledTimes(1);
		expect(onClearCompleted).toHaveBeenCalledTimes(1);
		expect(onToggle).toHaveBeenCalledTimes(1);
	});

	it("exposes upload settings controls", () => {
		const onConcurrencyChange = vi.fn();
		const onAutoClearCompletedChange = vi.fn();

		render(
			<UploadPanel
				open
				onToggle={vi.fn()}
				title="Uploads"
				summary="1 task"
				tasks={[]}
				emptyText="No tasks"
				concurrency={2}
				autoClearCompleted={false}
				onConcurrencyChange={onConcurrencyChange}
				onAutoClearCompletedChange={onAutoClearCompletedChange}
			/>,
		);

		fireEvent.click(screen.getByRole("button", { name: "upload_settings" }));

		expect(screen.getByText("upload_concurrency")).toBeInTheDocument();
		expect(screen.getByText("upload_concurrency_desc")).toBeInTheDocument();
		expect(screen.getByText("upload_auto_clear_completed")).toBeInTheDocument();
		expect(
			screen.getByText("upload_auto_clear_completed_desc"),
		).toBeInTheDocument();

		fireEvent.click(
			screen.getByRole("button", { name: "upload_concurrency_increase" }),
		);
		fireEvent.click(
			screen.getByRole("button", { name: "upload_concurrency_decrease" }),
		);
		fireEvent.click(
			screen.getByRole("switch", { name: "upload_auto_clear_completed" }),
		);

		expect(onConcurrencyChange).toHaveBeenNthCalledWith(1, 3);
		expect(onConcurrencyChange).toHaveBeenNthCalledWith(2, 1);
		expect(onAutoClearCompletedChange).toHaveBeenCalledWith(true);
	});

	it("opens the upload panel when settings are requested while collapsed", () => {
		const onToggle = vi.fn();

		render(
			<UploadPanel
				open={false}
				onToggle={onToggle}
				title="Uploads"
				summary="1 task"
				tasks={[]}
				emptyText="No tasks"
			/>,
		);

		fireEvent.click(screen.getByRole("button", { name: "upload_settings" }));

		expect(onToggle).toHaveBeenCalledTimes(1);
		expect(
			screen.getByRole("button", { name: "upload_settings" }),
		).toHaveAttribute("aria-expanded", "true");
	});

	it("hides progress chrome when all uploads are completed", () => {
		render(
			<UploadPanel
				open
				onToggle={vi.fn()}
				title="Uploads"
				summary="2 tasks · all complete"
				tasks={[
					{
						id: "done-a",
						title: "done-a.txt",
						status: "Done",
						mode: "Direct",
						progress: 100,
						completed: true,
					},
					{
						id: "done-b",
						title: "done-b.txt",
						status: "Done",
						mode: "Direct",
						progress: 100,
						completed: true,
					},
				]}
				emptyText="No tasks"
				totalCount={2}
				successCount={2}
				failedCount={0}
				activeCount={0}
				overallProgress={100}
			/>,
		);

		expect(screen.getByText("2 tasks · all complete")).toBeInTheDocument();
		expect(screen.queryByText("100%")).not.toBeInTheDocument();
	});

	it("does not treat cancelled uploads as active rows", () => {
		render(
			<UploadPanel
				open
				onToggle={vi.fn()}
				title="Uploads"
				summary="1 task · cancelled"
				tasks={[
					{
						id: "cancelled-a",
						title: "cancelled-a.txt",
						status: "Upload canceled",
						mode: "Chunked",
						progress: 42,
						cancelled: true,
					},
				]}
				emptyText="No tasks"
				totalCount={1}
				successCount={0}
				failedCount={0}
				activeCount={0}
				overallProgress={42}
			/>,
		);

		expect(screen.getByText(/Upload canceled/)).toBeInTheDocument();
		expect(screen.getByText("upload_batch_partial_failed")).toBeInTheDocument();
		expect(screen.queryByText("42%")).not.toBeInTheDocument();
	});
});
