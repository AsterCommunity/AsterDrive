import {
	act,
	fireEvent,
	render,
	screen,
	waitFor,
	within,
} from "@testing-library/react";
import { beforeEach, describe, expect, it, vi } from "vitest";
import { VersionHistoryDialog } from "@/components/files/VersionHistoryDialog";
import type { FileVersion } from "@/types/api";

const mockState = vi.hoisted(() => ({
	deleteVersion: vi.fn(),
	handleApiError: vi.fn(),
	invalidateFileResourceCachesForMutation: vi.fn(),
	listVersions: vi.fn(),
	restoreVersion: vi.fn(),
	toastSuccess: vi.fn(),
}));

vi.mock("react-i18next", () => ({
	useTranslation: () => ({
		t: (key: string, opts?: Record<string, unknown>) => {
			if (key === "version_history_title") {
				return `history:${opts?.name}`;
			}
			if (key === "version_history_count") {
				return `count:${opts?.count}`;
			}
			if (key === "version_restore_confirm_desc") {
				return `restore:${opts?.version}`;
			}
			if (key === "version_delete_confirm_desc") {
				return `delete:${opts?.version}`;
			}
			if (key === "loading_preview") {
				return "loading";
			}
			return key;
		},
	}),
}));

vi.mock("sonner", () => ({
	toast: {
		success: (...args: unknown[]) => mockState.toastSuccess(...args),
	},
}));

vi.mock("@/components/files/FileTypeIcon", () => ({
	FileTypeIcon: ({
		fileName,
		mimeType,
	}: {
		fileName: string;
		mimeType: string;
	}) => (
		<span data-testid="file-type-icon">
			{mimeType}:{fileName}
		</span>
	),
}));

vi.mock("@/components/ui/button", () => ({
	Button: ({
		children,
		title,
		disabled,
		onClick,
		className,
	}: {
		children: React.ReactNode;
		title?: string;
		disabled?: boolean;
		onClick?: () => void;
		className?: string;
	}) => (
		<button
			type="button"
			aria-label={title}
			disabled={disabled}
			onClick={onClick}
			className={className}
		>
			{children}
		</button>
	),
}));

vi.mock("@/components/ui/dialog", () => ({
	Dialog: ({
		children,
		onOpenChange,
		open,
	}: {
		children: React.ReactNode;
		onOpenChange: (open: boolean) => void;
		open: boolean;
	}) => (
		<div data-testid="dialog" hidden={!open}>
			<button
				type="button"
				aria-label="dialog_close_control"
				onClick={() => onOpenChange(false)}
			/>
			{children}
		</div>
	),
	DialogContent: ({
		children,
		className,
	}: {
		children: React.ReactNode;
		className?: string;
	}) => <div className={className}>{children}</div>,
	DialogHeader: ({ children }: { children: React.ReactNode }) => (
		<div>{children}</div>
	),
	DialogTitle: ({ children }: { children: React.ReactNode }) => (
		<h2>{children}</h2>
	),
}));

vi.mock("@/components/ui/icon", () => ({
	Icon: ({ name, className }: { name: string; className?: string }) => (
		<span data-testid="icon" data-name={name} className={className}>
			{name}
		</span>
	),
}));

vi.mock("@/components/ui/table", () => ({
	Table: ({ children }: { children: React.ReactNode }) => (
		<table>{children}</table>
	),
	TableHeader: ({ children }: { children: React.ReactNode }) => (
		<thead>{children}</thead>
	),
	TableBody: ({ children }: { children: React.ReactNode }) => (
		<tbody>{children}</tbody>
	),
	TableRow: ({ children }: { children: React.ReactNode }) => (
		<tr>{children}</tr>
	),
	TableHead: ({ children }: { children?: React.ReactNode }) => (
		<th>{children}</th>
	),
	TableCell: ({
		children,
		className,
	}: {
		children?: React.ReactNode;
		className?: string;
	}) => <td className={className}>{children}</td>,
}));

vi.mock("@/hooks/useApiError", () => ({
	handleApiError: (...args: unknown[]) => mockState.handleApiError(...args),
}));

vi.mock("@/lib/fileResourceCacheInvalidation", () => ({
	invalidateFileResourceCachesForMutation: (...args: unknown[]) =>
		mockState.invalidateFileResourceCachesForMutation(...args),
}));

vi.mock("@/lib/format", () => ({
	formatBytes: (value: number) => `bytes:${value}`,
	formatDateTime: (value: string) => `time:${value}`,
}));

vi.mock("@/services/fileService", () => ({
	fileService: {
		listVersions: (...args: unknown[]) => mockState.listVersions(...args),
		restoreVersion: (...args: unknown[]) => mockState.restoreVersion(...args),
		deleteVersion: (...args: unknown[]) => mockState.deleteVersion(...args),
		downloadPath: (fileId: number) => `/files/${fileId}/download`,
		thumbnailPath: (fileId: number) => `/files/${fileId}/thumbnail`,
		imagePreviewPath: (fileId: number) => `/files/${fileId}/image-preview`,
	},
}));

const versions: FileVersion[] = [
	{
		blob_id: 22,
		comment: null,
		created_at: "2026-03-03T00:00:00Z",
		creator_display_name: "alice",
		creator_user_id: 7,
		current: true,
		etag: "etag-3",
		file_id: 8,
		id: 13,
		mime_type: "application/pdf",
		public_id: "revision-3",
		reason: "overwrite",
		size: 128,
		version: 3,
	},
	{
		blob_id: 21,
		comment: null,
		created_at: "2026-03-01T00:00:00Z",
		creator_display_name: "alice",
		creator_user_id: 7,
		current: false,
		etag: "etag-2",
		file_id: 8,
		id: 11,
		mime_type: "application/pdf",
		public_id: "revision-2",
		reason: "overwrite",
		size: 256,
		version: 2,
	},
	{
		blob_id: 20,
		comment: null,
		created_at: "2026-02-28T00:00:00Z",
		creator_display_name: "alice",
		creator_user_id: 7,
		current: false,
		etag: "etag-1",
		file_id: 8,
		id: 12,
		mime_type: "application/pdf",
		public_id: "revision-1",
		reason: "create",
		size: 64,
		version: 1,
	},
];

function versionPage(
	items: FileVersion[],
	nextAfterSequence: number | null = null,
) {
	return { items, nextAfterSequence };
}

function deferred<T>() {
	let resolve!: (value: T) => void;
	let reject!: (reason: unknown) => void;
	const promise = new Promise<T>((resolvePromise, rejectPromise) => {
		resolve = resolvePromise;
		reject = rejectPromise;
	});
	return { promise, reject, resolve };
}

async function confirmVersionAction(action: "restore" | "delete") {
	const versionRow = (await screen.findByText("v2")).closest("tr");
	expect(versionRow).not.toBeNull();
	fireEvent.click(
		within(versionRow as HTMLTableRowElement).getByRole("button", {
			name: `version_${action}`,
		}),
	);
	const confirmRow = screen.getByText(`${action}:2`).closest("tr");
	expect(confirmRow).not.toBeNull();
	fireEvent.click(
		within(confirmRow as HTMLTableRowElement).getByRole("button", {
			name: `version_${action}`,
		}),
	);
}

describe("VersionHistoryDialog", () => {
	beforeEach(() => {
		mockState.deleteVersion.mockReset();
		mockState.handleApiError.mockReset();
		mockState.invalidateFileResourceCachesForMutation.mockReset();
		mockState.listVersions.mockReset();
		mockState.restoreVersion.mockReset();
		mockState.toastSuccess.mockReset();
	});

	it("shows loading state, renders version rows, and clears them when closed", async () => {
		let resolveList:
			| ((value: ReturnType<typeof versionPage>) => void)
			| undefined;

		mockState.listVersions.mockImplementationOnce(
			() =>
				new Promise<ReturnType<typeof versionPage>>((resolve) => {
					resolveList = resolve;
				}),
		);

		const { rerender } = render(
			<VersionHistoryDialog
				open
				onOpenChange={vi.fn()}
				fileId={8}
				fileName="report.pdf"
				mimeType="application/pdf"
			/>,
		);

		expect(mockState.listVersions).toHaveBeenCalledWith(8);
		expect(
			screen.getByRole("heading", { name: "history:report.pdf" }),
		).toBeInTheDocument();
		expect(screen.queryByText("application/pdf · bytes:2048")).toBeNull();
		expect(screen.queryByText("bytes:2048 · application/pdf")).toBeNull();
		expect(screen.getByText("count:0")).toBeInTheDocument();
		expect(screen.getByText("loading")).toBeInTheDocument();
		expect(screen.getAllByTestId("file-type-icon")).toHaveLength(1);

		resolveList?.(versionPage(versions));

		expect(await screen.findAllByText("v3")).toHaveLength(2);
		expect(screen.getByText("v2")).toBeInTheDocument();
		expect(screen.getByText("v1")).toBeInTheDocument();
		expect(screen.getByText("bytes:128")).toBeInTheDocument();
		expect(screen.getByText("bytes:256")).toBeInTheDocument();
		expect(screen.getByText("time:2026-03-01T00:00:00Z")).toBeInTheDocument();
		expect(screen.getByText("count:2")).toBeInTheDocument();
		const currentRow = screen
			.getAllByText("v3")
			.find((element) => element.closest("tr"))
			?.closest("tr");
		expect(currentRow).not.toBeNull();
		expect(
			within(currentRow as HTMLTableRowElement).queryByRole("button"),
		).toBeNull();

		rerender(
			<VersionHistoryDialog
				open={false}
				onOpenChange={vi.fn()}
				fileId={8}
				fileName="report.pdf"
				mimeType="application/pdf"
			/>,
		);

		expect(screen.getByTestId("dialog")).not.toBeVisible();
		expect(screen.queryByText("v2")).not.toBeInTheDocument();
	});

	it("loads older revisions on demand without replacing the current page", async () => {
		mockState.listVersions
			.mockResolvedValueOnce(versionPage(versions.slice(0, 2), 2))
			.mockResolvedValueOnce(versionPage([versions[2]]));

		render(
			<VersionHistoryDialog
				open
				onOpenChange={vi.fn()}
				fileId={8}
				fileName="report.pdf"
			/>,
		);

		await screen.findByText("v2");
		expect(screen.queryByText("v1")).not.toBeInTheDocument();
		fireEvent.click(screen.getByRole("button", { name: "version_load_more" }));

		expect(await screen.findByText("v1")).toBeInTheDocument();
		expect(screen.getByText("v2")).toBeInTheDocument();
		expect(mockState.listVersions).toHaveBeenNthCalledWith(1, 8);
		expect(mockState.listVersions).toHaveBeenNthCalledWith(2, 8, 2);
		expect(
			screen.queryByRole("button", { name: "version_load_more" }),
		).not.toBeInTheDocument();
	});

	it("keeps the loaded page and re-enables pagination when loading more fails", async () => {
		const error = new Error("older page failed");
		mockState.listVersions
			.mockResolvedValueOnce(versionPage(versions.slice(0, 2), 2))
			.mockRejectedValueOnce(error);

		render(
			<VersionHistoryDialog
				open
				onOpenChange={vi.fn()}
				fileId={8}
				fileName="report.pdf"
			/>,
		);

		const loadMore = await screen.findByRole("button", {
			name: "version_load_more",
		});
		fireEvent.click(loadMore);
		await waitFor(() => {
			expect(mockState.handleApiError).toHaveBeenCalledWith(error);
		});
		expect(screen.getByText("v2")).toBeInTheDocument();
		expect(loadMore).toBeEnabled();
	});

	it("discards stale pagination results and errors after the dialog closes", async () => {
		const resolvedOlderPage = deferred<ReturnType<typeof versionPage>>();
		const rejectedOlderPage = deferred<ReturnType<typeof versionPage>>();
		mockState.listVersions
			.mockResolvedValueOnce(versionPage(versions.slice(0, 2), 2))
			.mockReturnValueOnce(resolvedOlderPage.promise)
			.mockResolvedValueOnce(versionPage(versions.slice(0, 2), 2))
			.mockReturnValueOnce(rejectedOlderPage.promise);

		const { rerender } = render(
			<VersionHistoryDialog
				open
				onOpenChange={vi.fn()}
				fileId={8}
				fileName="report.pdf"
			/>,
		);

		fireEvent.click(
			await screen.findByRole("button", { name: "version_load_more" }),
		);
		rerender(
			<VersionHistoryDialog
				open={false}
				onOpenChange={vi.fn()}
				fileId={8}
				fileName="report.pdf"
			/>,
		);
		await act(async () => {
			resolvedOlderPage.resolve(versionPage([versions[2]]));
			await resolvedOlderPage.promise;
		});
		expect(screen.queryByText("v1")).not.toBeInTheDocument();
		expect(screen.getByText("version_empty")).toBeInTheDocument();

		rerender(
			<VersionHistoryDialog
				open
				onOpenChange={vi.fn()}
				fileId={8}
				fileName="report.pdf"
			/>,
		);
		fireEvent.click(
			await screen.findByRole("button", { name: "version_load_more" }),
		);
		rerender(
			<VersionHistoryDialog
				open={false}
				onOpenChange={vi.fn()}
				fileId={8}
				fileName="report.pdf"
			/>,
		);
		await act(async () => {
			rejectedOlderPage.reject(new Error("stale older page"));
			await rejectedOlderPage.promise.catch(() => undefined);
		});

		expect(mockState.handleApiError).not.toHaveBeenCalled();
		expect(screen.queryByText("v1")).not.toBeInTheDocument();
		expect(screen.getByText("version_empty")).toBeInTheDocument();
	});

	it("blocks revision mutations while an older page is loading", async () => {
		let resolveOlderPage:
			| ((value: ReturnType<typeof versionPage>) => void)
			| undefined;
		mockState.listVersions
			.mockResolvedValueOnce(versionPage(versions.slice(0, 2), 2))
			.mockImplementationOnce(
				() =>
					new Promise<ReturnType<typeof versionPage>>((resolve) => {
						resolveOlderPage = resolve;
					}),
			);

		render(
			<VersionHistoryDialog
				open
				onOpenChange={vi.fn()}
				fileId={8}
				fileName="report.pdf"
			/>,
		);

		const versionRow = (await screen.findByText("v2")).closest("tr");
		expect(versionRow).not.toBeNull();
		fireEvent.click(screen.getByRole("button", { name: "version_load_more" }));

		expect(
			within(versionRow as HTMLTableRowElement).getByRole("button", {
				name: "version_restore",
			}),
		).toBeDisabled();
		expect(
			within(versionRow as HTMLTableRowElement).getByRole("button", {
				name: "version_delete",
			}),
		).toBeDisabled();

		resolveOlderPage?.(versionPage([versions[2]]));
		expect(await screen.findByText("v1")).toBeInTheDocument();
	});

	it("restores a version after confirmation and invalidates related caches", async () => {
		const onRestored = vi.fn();
		const restoredVersions: FileVersion[] = [
			{
				...versions[1],
				blob_id: versions[1].blob_id,
				current: true,
				etag: "etag-4",
				id: 14,
				public_id: "revision-4",
				reason: "restore",
				version: 4,
			},
			{ ...versions[0], current: false },
			versions[1],
			versions[2],
		];
		mockState.listVersions.mockResolvedValueOnce(versionPage(versions));
		mockState.restoreVersion.mockResolvedValueOnce(undefined);
		mockState.listVersions.mockResolvedValueOnce(versionPage(restoredVersions));

		render(
			<VersionHistoryDialog
				open
				onOpenChange={vi.fn()}
				fileId={10}
				fileName="diagram.png"
				onRestored={onRestored}
			/>,
		);

		await screen.findByText("v2");
		const versionRow = screen.getByText("v2").closest("tr");
		expect(versionRow).not.toBeNull();
		fireEvent.click(
			within(versionRow as HTMLTableRowElement).getByRole("button", {
				name: "version_restore",
			}),
		);

		expect(screen.queryByTestId("confirm-dialog")).not.toBeInTheDocument();
		expect(screen.getByText("restore:2")).toBeInTheDocument();

		const inlineConfirmRow = screen.getByText("restore:2").closest("tr");
		expect(inlineConfirmRow).not.toBeNull();
		fireEvent.click(
			within(inlineConfirmRow as HTMLTableRowElement).getByRole("button", {
				name: "version_restore",
			}),
		);

		await waitFor(() => {
			expect(mockState.restoreVersion).toHaveBeenCalledWith(10, 11);
		});
		expect(await screen.findAllByText("v4")).toHaveLength(2);
		expect(screen.getByText("count:3")).toBeInTheDocument();
		expect(mockState.listVersions).toHaveBeenCalledTimes(2);
		expect(
			mockState.invalidateFileResourceCachesForMutation,
		).toHaveBeenCalledWith({
			download: "/files/10/download",
			thumbnail: "/files/10/thumbnail",
			imagePreview: "/files/10/image-preview",
		});
		expect(mockState.toastSuccess).toHaveBeenCalledWith("version_restored");
		expect(onRestored).toHaveBeenCalledTimes(1);
	});

	it("deletes a version after confirmation and removes it from the rendered list", async () => {
		mockState.listVersions.mockResolvedValueOnce(versionPage(versions));
		mockState.deleteVersion.mockResolvedValueOnce(undefined);
		mockState.listVersions.mockResolvedValueOnce(
			versionPage([versions[0], versions[2]]),
		);

		render(
			<VersionHistoryDialog
				open
				onOpenChange={vi.fn()}
				fileId={15}
				fileName="archive.zip"
			/>,
		);

		await screen.findByText("v2");
		const versionRow = screen.getByText("v2").closest("tr");
		expect(versionRow).not.toBeNull();
		fireEvent.click(
			within(versionRow as HTMLTableRowElement).getByRole("button", {
				name: "version_delete",
			}),
		);

		expect(screen.queryByTestId("confirm-dialog")).not.toBeInTheDocument();
		expect(screen.getByText("delete:2")).toBeInTheDocument();

		const inlineConfirmRow = screen.getByText("delete:2").closest("tr");
		expect(inlineConfirmRow).not.toBeNull();
		fireEvent.click(
			within(inlineConfirmRow as HTMLTableRowElement).getByRole("button", {
				name: "version_delete",
			}),
		);

		await waitFor(() => {
			expect(mockState.deleteVersion).toHaveBeenCalledWith(15, 11);
		});
		expect(mockState.toastSuccess).toHaveBeenCalledWith("version_deleted");
		expect(screen.queryByText("v2")).toBeNull();
		expect(screen.getByText("v1")).toBeInTheDocument();
	});

	it("clears inline confirmation and closes through the dialog callback", async () => {
		const onOpenChange = vi.fn();
		mockState.listVersions.mockResolvedValueOnce(versionPage(versions));

		render(
			<VersionHistoryDialog
				open
				onOpenChange={onOpenChange}
				fileId={16}
				fileName="report.pdf"
			/>,
		);

		const versionRow = (await screen.findByText("v2")).closest("tr");
		fireEvent.click(
			within(versionRow as HTMLTableRowElement).getByRole("button", {
				name: "version_restore",
			}),
		);
		fireEvent.click(screen.getByRole("button", { name: "core:cancel" }));
		expect(screen.queryByText("restore:2")).not.toBeInTheDocument();

		fireEvent.click(
			screen.getByRole("button", { name: "dialog_close_control" }),
		);
		expect(onOpenChange).toHaveBeenCalledWith(false);
		expect(screen.getByText("version_empty")).toBeInTheDocument();
	});

	it("keeps successful mutation side effects but ignores stale completion state", async () => {
		const restoreRequest = deferred<void>();
		const deleteRequest = deferred<void>();
		const onRestored = vi.fn();
		mockState.listVersions.mockResolvedValue(versionPage(versions));
		mockState.restoreVersion.mockReturnValueOnce(restoreRequest.promise);
		mockState.deleteVersion.mockReturnValueOnce(deleteRequest.promise);

		const restoreView = render(
			<VersionHistoryDialog
				open
				onOpenChange={vi.fn()}
				fileId={22}
				fileName="report.pdf"
				onRestored={onRestored}
			/>,
		);
		await confirmVersionAction("restore");
		restoreView.rerender(
			<VersionHistoryDialog
				open={false}
				onOpenChange={vi.fn()}
				fileId={22}
				fileName="report.pdf"
				onRestored={onRestored}
			/>,
		);
		await act(async () => {
			restoreRequest.resolve();
			await restoreRequest.promise;
		});
		expect(mockState.toastSuccess).toHaveBeenCalledWith("version_restored");
		expect(onRestored).toHaveBeenCalledTimes(1);
		expect(mockState.listVersions).toHaveBeenCalledTimes(1);
		restoreView.unmount();

		const deleteView = render(
			<VersionHistoryDialog
				open
				onOpenChange={vi.fn()}
				fileId={23}
				fileName="report.pdf"
			/>,
		);
		await confirmVersionAction("delete");
		deleteView.rerender(
			<VersionHistoryDialog
				open={false}
				onOpenChange={vi.fn()}
				fileId={23}
				fileName="report.pdf"
			/>,
		);
		await act(async () => {
			deleteRequest.resolve();
			await deleteRequest.promise;
		});
		expect(mockState.toastSuccess).toHaveBeenCalledWith("version_deleted");
		expect(mockState.listVersions).toHaveBeenCalledTimes(2);
	});

	it("does not surface stale mutation failures after the dialog closes", async () => {
		const restoreRequest = deferred<void>();
		const deleteRequest = deferred<void>();
		mockState.listVersions.mockResolvedValue(versionPage(versions));
		mockState.restoreVersion.mockReturnValueOnce(restoreRequest.promise);
		mockState.deleteVersion.mockReturnValueOnce(deleteRequest.promise);

		const restoreView = render(
			<VersionHistoryDialog
				open
				onOpenChange={vi.fn()}
				fileId={24}
				fileName="report.pdf"
			/>,
		);
		await confirmVersionAction("restore");
		restoreView.rerender(
			<VersionHistoryDialog
				open={false}
				onOpenChange={vi.fn()}
				fileId={24}
				fileName="report.pdf"
			/>,
		);
		await act(async () => {
			restoreRequest.reject(new Error("stale restore"));
			await restoreRequest.promise.catch(() => undefined);
		});
		restoreView.unmount();

		const deleteView = render(
			<VersionHistoryDialog
				open
				onOpenChange={vi.fn()}
				fileId={25}
				fileName="report.pdf"
			/>,
		);
		await confirmVersionAction("delete");
		deleteView.rerender(
			<VersionHistoryDialog
				open={false}
				onOpenChange={vi.fn()}
				fileId={25}
				fileName="report.pdf"
			/>,
		);
		await act(async () => {
			deleteRequest.reject(new Error("stale delete"));
			await deleteRequest.promise.catch(() => undefined);
		});

		expect(mockState.handleApiError).not.toHaveBeenCalled();
		expect(mockState.toastSuccess).not.toHaveBeenCalled();
	});

	it("ignores stale restore and delete refresh responses", async () => {
		const restoreRefresh = deferred<ReturnType<typeof versionPage>>();
		const deleteRefresh = deferred<ReturnType<typeof versionPage>>();
		mockState.listVersions
			.mockResolvedValueOnce(versionPage(versions))
			.mockReturnValueOnce(restoreRefresh.promise)
			.mockResolvedValueOnce(versionPage(versions))
			.mockReturnValueOnce(deleteRefresh.promise)
			.mockResolvedValueOnce(versionPage(versions));
		mockState.restoreVersion.mockResolvedValueOnce(undefined);
		mockState.deleteVersion.mockResolvedValueOnce(undefined);

		const restoreView = render(
			<VersionHistoryDialog
				open
				onOpenChange={vi.fn()}
				fileId={26}
				fileName="report.pdf"
			/>,
		);
		await confirmVersionAction("restore");
		await waitFor(() =>
			expect(mockState.listVersions).toHaveBeenCalledTimes(2),
		);
		restoreView.rerender(
			<VersionHistoryDialog
				open={false}
				onOpenChange={vi.fn()}
				fileId={26}
				fileName="report.pdf"
			/>,
		);
		await act(async () => {
			restoreRefresh.reject(new Error("stale restore refresh"));
			await restoreRefresh.promise.catch(() => undefined);
		});
		restoreView.unmount();

		const deleteView = render(
			<VersionHistoryDialog
				open
				onOpenChange={vi.fn()}
				fileId={27}
				fileName="report.pdf"
			/>,
		);
		await confirmVersionAction("delete");
		await waitFor(() =>
			expect(mockState.listVersions).toHaveBeenCalledTimes(4),
		);
		deleteView.rerender(
			<VersionHistoryDialog
				open={false}
				onOpenChange={vi.fn()}
				fileId={27}
				fileName="report.pdf"
			/>,
		);
		await act(async () => {
			deleteRefresh.resolve(versionPage([versions[0], versions[2]]));
			await deleteRefresh.promise;
		});

		expect(mockState.handleApiError).not.toHaveBeenCalled();
		expect(screen.queryByText("v1")).not.toBeInTheDocument();
		expect(screen.getByText("version_empty")).toBeInTheDocument();

		deleteView.rerender(
			<VersionHistoryDialog
				open
				onOpenChange={vi.fn()}
				fileId={27}
				fileName="report.pdf"
			/>,
		);
		expect(await screen.findByText("v2")).toBeInTheDocument();
		expect(screen.queryByText("v1")).toBeInTheDocument();
	});

	it("keeps the loaded history and re-enables actions when restore fails", async () => {
		const error = new Error("restore failed");
		mockState.listVersions.mockResolvedValueOnce(versionPage(versions));
		mockState.restoreVersion.mockRejectedValueOnce(error);

		render(
			<VersionHistoryDialog
				open
				onOpenChange={vi.fn()}
				fileId={18}
				fileName="report.pdf"
			/>,
		);

		const versionRow = (await screen.findByText("v2")).closest("tr");
		expect(versionRow).not.toBeNull();
		fireEvent.click(
			within(versionRow as HTMLTableRowElement).getByRole("button", {
				name: "version_restore",
			}),
		);
		const inlineConfirmRow = screen.getByText("restore:2").closest("tr");
		expect(inlineConfirmRow).not.toBeNull();
		fireEvent.click(
			within(inlineConfirmRow as HTMLTableRowElement).getByRole("button", {
				name: "version_restore",
			}),
		);

		await waitFor(() => {
			expect(mockState.handleApiError).toHaveBeenCalledWith(error);
		});
		expect(screen.getByText("v2")).toBeInTheDocument();
		expect(
			within(versionRow as HTMLTableRowElement).getByRole("button", {
				name: "version_restore",
			}),
		).toBeEnabled();
		expect(mockState.listVersions).toHaveBeenCalledTimes(1);
		expect(mockState.toastSuccess).not.toHaveBeenCalled();
	});

	it("keeps the loaded history and re-enables actions when delete fails", async () => {
		const error = new Error("delete failed");
		mockState.listVersions.mockResolvedValueOnce(versionPage(versions));
		mockState.deleteVersion.mockRejectedValueOnce(error);

		render(
			<VersionHistoryDialog
				open
				onOpenChange={vi.fn()}
				fileId={19}
				fileName="report.pdf"
			/>,
		);

		await confirmVersionAction("delete");
		await waitFor(() => {
			expect(mockState.handleApiError).toHaveBeenCalledWith(error);
		});
		const versionRow = screen.getByText("v2").closest("tr");
		expect(
			within(versionRow as HTMLTableRowElement).getByRole("button", {
				name: "version_delete",
			}),
		).toBeEnabled();
		expect(mockState.listVersions).toHaveBeenCalledTimes(1);
		expect(mockState.toastSuccess).not.toHaveBeenCalled();
	});

	it("keeps successful restore side effects when refreshing history fails", async () => {
		const refreshError = new Error("refresh failed");
		const onRestored = vi.fn();
		mockState.listVersions.mockResolvedValueOnce(versionPage(versions));
		mockState.restoreVersion.mockResolvedValueOnce(undefined);
		mockState.listVersions.mockRejectedValueOnce(refreshError);

		render(
			<VersionHistoryDialog
				open
				onOpenChange={vi.fn()}
				fileId={20}
				fileName="report.pdf"
				onRestored={onRestored}
			/>,
		);

		const versionRow = (await screen.findByText("v2")).closest("tr");
		fireEvent.click(
			within(versionRow as HTMLTableRowElement).getByRole("button", {
				name: "version_restore",
			}),
		);
		const confirmRow = screen.getByText("restore:2").closest("tr");
		fireEvent.click(
			within(confirmRow as HTMLTableRowElement).getByRole("button", {
				name: "version_restore",
			}),
		);

		await waitFor(() => {
			expect(mockState.handleApiError).toHaveBeenCalledWith(refreshError);
		});
		expect(mockState.toastSuccess).toHaveBeenCalledWith("version_restored");
		expect(onRestored).toHaveBeenCalledTimes(1);
		expect(
			mockState.invalidateFileResourceCachesForMutation,
		).toHaveBeenCalledTimes(1);
		expect(screen.getByText("v2")).toBeInTheDocument();
	});

	it("keeps successful delete state usable when refreshing history fails", async () => {
		const refreshError = new Error("refresh failed");
		mockState.listVersions.mockResolvedValueOnce(versionPage(versions));
		mockState.deleteVersion.mockResolvedValueOnce(undefined);
		mockState.listVersions.mockRejectedValueOnce(refreshError);

		render(
			<VersionHistoryDialog
				open
				onOpenChange={vi.fn()}
				fileId={21}
				fileName="report.pdf"
			/>,
		);

		const versionRow = (await screen.findByText("v2")).closest("tr");
		fireEvent.click(
			within(versionRow as HTMLTableRowElement).getByRole("button", {
				name: "version_delete",
			}),
		);
		const confirmRow = screen.getByText("delete:2").closest("tr");
		fireEvent.click(
			within(confirmRow as HTMLTableRowElement).getByRole("button", {
				name: "version_delete",
			}),
		);

		await waitFor(() => {
			expect(mockState.handleApiError).toHaveBeenCalledWith(refreshError);
		});
		expect(mockState.toastSuccess).toHaveBeenCalledWith("version_deleted");
		expect(screen.getByText("v2")).toBeInTheDocument();
		expect(
			within(versionRow as HTMLTableRowElement).getByRole("button", {
				name: "version_delete",
			}),
		).toBeEnabled();
	});

	it("ignores an initial response that belongs to a previous file", async () => {
		let resolveOld:
			| ((value: ReturnType<typeof versionPage>) => void)
			| undefined;
		mockState.listVersions
			.mockImplementationOnce(
				() =>
					new Promise<ReturnType<typeof versionPage>>((resolve) => {
						resolveOld = resolve;
					}),
			)
			.mockResolvedValueOnce(versionPage([{ ...versions[0], file_id: 9 }]));

		const { rerender } = render(
			<VersionHistoryDialog
				open
				onOpenChange={vi.fn()}
				fileId={8}
				fileName="old.pdf"
			/>,
		);
		rerender(
			<VersionHistoryDialog
				open
				onOpenChange={vi.fn()}
				fileId={9}
				fileName="new.pdf"
			/>,
		);

		expect(await screen.findAllByText("v3")).toHaveLength(2);
		resolveOld?.(versionPage(versions));
		await act(async () => {
			await Promise.resolve();
			await Promise.resolve();
		});
		expect(mockState.listVersions).toHaveBeenNthCalledWith(2, 9);
		expect(screen.getAllByText("v3")).toHaveLength(2);
		expect(screen.getByText("count:0")).toBeInTheDocument();
	});

	it("surfaces loading failures through the api error handler and falls back to the empty state", async () => {
		const error = new Error("network");
		mockState.listVersions.mockRejectedValueOnce(error);

		render(
			<VersionHistoryDialog
				open
				onOpenChange={vi.fn()}
				fileId={99}
				fileName="broken.txt"
			/>,
		);

		await waitFor(() => {
			expect(mockState.handleApiError).toHaveBeenCalledWith(error);
		});
		expect(screen.getByText("version_empty")).toBeInTheDocument();
	});
});
