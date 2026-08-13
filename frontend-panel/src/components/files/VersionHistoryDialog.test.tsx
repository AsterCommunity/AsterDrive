import {
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
	Dialog: ({ children, open }: { children: React.ReactNode; open: boolean }) =>
		open ? <div data-testid="dialog">{children}</div> : null,
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
		let resolveList: ((value: typeof versions) => void) | undefined;

		mockState.listVersions.mockImplementationOnce(
			() =>
				new Promise<typeof versions>((resolve) => {
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

		resolveList?.(versions);

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

		expect(screen.queryByTestId("dialog")).not.toBeInTheDocument();
		expect(screen.queryByText("v2")).not.toBeInTheDocument();
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
		mockState.listVersions.mockResolvedValueOnce(versions);
		mockState.restoreVersion.mockResolvedValueOnce(undefined);
		mockState.listVersions.mockResolvedValueOnce(restoredVersions);

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
		mockState.listVersions.mockResolvedValueOnce(versions);
		mockState.deleteVersion.mockResolvedValueOnce(undefined);
		mockState.listVersions.mockResolvedValueOnce([versions[0], versions[2]]);

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

	it("keeps the loaded history and re-enables actions when restore fails", async () => {
		const error = new Error("restore failed");
		mockState.listVersions.mockResolvedValueOnce(versions);
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
