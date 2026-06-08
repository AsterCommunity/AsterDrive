import {
	fireEvent,
	render,
	screen,
	waitFor,
	within,
} from "@testing-library/react";
import { beforeEach, describe, expect, it, vi } from "vitest";
import { VersionHistoryDialog } from "@/components/files/VersionHistoryDialog";

const mockState = vi.hoisted(() => ({
	deleteVersion: vi.fn(),
	handleApiError: vi.fn(),
	invalidateBlobUrl: vi.fn(),
	invalidateTextContent: vi.fn(),
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

vi.mock("@/hooks/useBlobUrl", () => ({
	invalidateBlobUrl: (...args: unknown[]) =>
		mockState.invalidateBlobUrl(...args),
}));

vi.mock("@/hooks/useTextContent", () => ({
	invalidateTextContent: (...args: unknown[]) =>
		mockState.invalidateTextContent(...args),
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

const versions = [
	{
		created_at: "2026-03-01T00:00:00Z",
		id: 11,
		size: 128,
		version: 2,
	},
	{
		created_at: "2026-03-02T00:00:00Z",
		id: 12,
		size: 256,
		version: 1,
	},
] as never[];

describe("VersionHistoryDialog", () => {
	beforeEach(() => {
		mockState.deleteVersion.mockReset();
		mockState.handleApiError.mockReset();
		mockState.invalidateBlobUrl.mockReset();
		mockState.invalidateTextContent.mockReset();
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

		expect(await screen.findByText("v3")).toBeInTheDocument();
		expect(screen.getByText("v2")).toBeInTheDocument();
		expect(screen.getByText("v1")).toBeInTheDocument();
		expect(screen.getByText("bytes:128")).toBeInTheDocument();
		expect(screen.getByText("bytes:256")).toBeInTheDocument();
		expect(screen.getByText("time:2026-03-01T00:00:00Z")).toBeInTheDocument();
		expect(screen.getByText("count:2")).toBeInTheDocument();

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
		mockState.listVersions.mockResolvedValueOnce([versions[0]]);
		mockState.restoreVersion.mockResolvedValueOnce(undefined);

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
		expect(mockState.invalidateTextContent).toHaveBeenCalledWith(
			"/files/10/download",
		);
		expect(mockState.invalidateBlobUrl).toHaveBeenCalledWith(
			"/files/10/download",
		);
		expect(mockState.invalidateBlobUrl).toHaveBeenCalledWith(
			"/files/10/thumbnail",
		);
		expect(mockState.toastSuccess).toHaveBeenCalledWith("version_restored");
		expect(onRestored).toHaveBeenCalledTimes(1);
	});

	it("deletes a version after confirmation and removes it from the rendered list", async () => {
		mockState.listVersions.mockResolvedValueOnce(versions);
		mockState.deleteVersion.mockResolvedValueOnce(undefined);
		mockState.listVersions.mockResolvedValueOnce([versions[1]]);

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
		expect(screen.getAllByText("v2")).toHaveLength(1);
		expect(screen.getByText("v1")).toBeInTheDocument();
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
