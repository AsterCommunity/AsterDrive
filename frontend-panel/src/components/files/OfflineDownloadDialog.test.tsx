import {
	act,
	fireEvent,
	render,
	screen,
	waitFor,
} from "@testing-library/react";
import { beforeEach, describe, expect, it, vi } from "vitest";
import { OfflineDownloadDialog } from "@/components/files/OfflineDownloadDialog";
import { deferred } from "@/test/deferred";

const mockState = vi.hoisted(() => ({
	createOfflineDownloadTask: vi.fn(),
	handleApiError: vi.fn(),
	toastError: vi.fn(),
	toastSuccess: vi.fn(),
}));

vi.mock("react-i18next", () => ({
	useTranslation: () => ({
		t: (key: string) => key,
	}),
}));

vi.mock("sonner", () => ({
	toast: {
		error: (...args: unknown[]) => mockState.toastError(...args),
		success: (...args: unknown[]) => mockState.toastSuccess(...args),
	},
}));

vi.mock("@/components/ui/button", () => ({
	Button: ({
		children,
		disabled,
		onClick,
		type,
	}: {
		children: React.ReactNode;
		disabled?: boolean;
		onClick?: () => void;
		type?: "button" | "submit";
	}) => (
		<button type={type ?? "button"} disabled={disabled} onClick={onClick}>
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
		onOpenChange?: (open: boolean) => void;
		open: boolean;
	}) =>
		open ? (
			<div data-testid="dialog">
				<button type="button" onClick={() => onOpenChange?.(false)}>
					dialog-close
				</button>
				{children}
			</div>
		) : null,
	DialogContent: ({ children }: { children: React.ReactNode }) => (
		<div>{children}</div>
	),
	DialogDescription: ({ children }: { children: React.ReactNode }) => (
		<p>{children}</p>
	),
	DialogFooter: ({ children }: { children: React.ReactNode }) => (
		<div>{children}</div>
	),
	DialogHeader: ({ children }: { children: React.ReactNode }) => (
		<div>{children}</div>
	),
	DialogTitle: ({ children }: { children: React.ReactNode }) => (
		<h2>{children}</h2>
	),
}));

vi.mock("@/components/ui/icon", () => ({
	Icon: ({ name }: { name: string }) => <span>{`icon:${name}`}</span>,
}));

vi.mock("@/components/ui/input", () => ({
	Input: ({ ...props }: React.InputHTMLAttributes<HTMLInputElement>) => (
		<input {...props} />
	),
}));

vi.mock("@/components/ui/label", () => ({
	Label: ({
		children,
		htmlFor,
	}: {
		children: React.ReactNode;
		htmlFor?: string;
	}) => <label htmlFor={htmlFor}>{children}</label>,
}));

vi.mock("@/hooks/useApiError", () => ({
	handleApiError: (...args: unknown[]) => mockState.handleApiError(...args),
}));

vi.mock("@/services/fileService", () => ({
	fileService: {
		createOfflineDownloadTask: (...args: unknown[]) =>
			mockState.createOfflineDownloadTask(...args),
	},
}));

describe("OfflineDownloadDialog", () => {
	beforeEach(() => {
		mockState.createOfflineDownloadTask.mockReset();
		mockState.handleApiError.mockReset();
		mockState.toastError.mockReset();
		mockState.toastSuccess.mockReset();
		mockState.createOfflineDownloadTask.mockResolvedValue({
			display_name: "Download example.iso",
		});
	});

	it("creates an offline download task with trimmed optional fields", async () => {
		const onOpenChange = vi.fn();

		render(
			<OfflineDownloadDialog
				open
				onOpenChange={onOpenChange}
				targetFolderId={12}
				targetFolderName=" Projects "
			/>,
		);

		expect(screen.getByText("icon:FolderOpen")).toBeInTheDocument();
		expect(screen.getByText("Projects")).toBeInTheDocument();

		fireEvent.change(
			screen.getByLabelText("tasks:offline_download_url_label"),
			{
				target: { value: "  https://example.com/example.iso  " },
			},
		);
		fireEvent.change(
			screen.getByLabelText("tasks:offline_download_filename_label"),
			{
				target: { value: "  example.iso  " },
			},
		);
		fireEvent.change(
			screen.getByLabelText("tasks:offline_download_sha256_label"),
			{
				target: { value: "  abc123  " },
			},
		);
		fireEvent.click(
			screen.getByRole("button", {
				name: /tasks:offline_download_submit/,
			}),
		);

		await waitFor(() => {
			expect(mockState.createOfflineDownloadTask).toHaveBeenCalledWith({
				expected_sha256: "abc123",
				filename: "example.iso",
				target_folder_id: 12,
				url: "https://example.com/example.iso",
			});
		});
		expect(mockState.toastSuccess).toHaveBeenCalledWith(
			"tasks:task_created_success",
			{ description: "Download example.iso" },
		);
		expect(onOpenChange).toHaveBeenCalledWith(false);
	});

	it("uses root as the fallback target and sends blank optional fields as null", async () => {
		render(
			<OfflineDownloadDialog
				open
				onOpenChange={vi.fn()}
				targetFolderId={null}
				targetFolderName="  "
			/>,
		);

		expect(screen.getByText("icon:House")).toBeInTheDocument();
		expect(screen.getByText("tasks:summary_root_folder")).toBeInTheDocument();

		fireEvent.change(
			screen.getByLabelText("tasks:offline_download_url_label"),
			{
				target: { value: "https://example.com/root.bin" },
			},
		);
		fireEvent.click(
			screen.getByRole("button", {
				name: /tasks:offline_download_submit/,
			}),
		);

		await waitFor(() => {
			expect(mockState.createOfflineDownloadTask).toHaveBeenCalledWith({
				expected_sha256: null,
				filename: null,
				target_folder_id: null,
				url: "https://example.com/root.bin",
			});
		});
	});

	it("ignores blank submissions and reports creation failures", async () => {
		const error = new Error("offline download failed");
		mockState.createOfflineDownloadTask.mockRejectedValueOnce(error);

		render(
			<OfflineDownloadDialog
				open
				onOpenChange={vi.fn()}
				targetFolderId={null}
				targetFolderName={null}
			/>,
		);

		const urlInput = screen.getByLabelText("tasks:offline_download_url_label");
		fireEvent.change(urlInput, { target: { value: "   " } });
		fireEvent.submit(urlInput.closest("form") ?? urlInput);
		expect(mockState.createOfflineDownloadTask).not.toHaveBeenCalled();

		fireEvent.change(urlInput, {
			target: { value: "https://example.com/fail.bin" },
		});
		fireEvent.click(
			screen.getByRole("button", {
				name: /tasks:offline_download_submit/,
			}),
		);

		await waitFor(() => {
			expect(mockState.handleApiError).toHaveBeenCalledWith(error);
		});
		expect(mockState.toastSuccess).not.toHaveBeenCalled();
	});

	it("blocks dialog close requests while a task is being submitted", async () => {
		let resolveTask: (value: { display_name: string }) => void = () => {};
		mockState.createOfflineDownloadTask.mockImplementation(
			() =>
				new Promise((resolve) => {
					resolveTask = resolve;
				}),
		);
		const onOpenChange = vi.fn();

		render(
			<OfflineDownloadDialog
				open
				onOpenChange={onOpenChange}
				targetFolderId={null}
				targetFolderName={null}
			/>,
		);

		fireEvent.change(
			screen.getByLabelText("tasks:offline_download_url_label"),
			{
				target: { value: "https://example.com/slow.bin" },
			},
		);
		fireEvent.click(
			screen.getByRole("button", {
				name: /tasks:offline_download_submit/,
			}),
		);

		await waitFor(() => {
			expect(screen.getByText("icon:Spinner")).toBeInTheDocument();
		});
		fireEvent.click(screen.getByRole("button", { name: "dialog-close" }));
		expect(onOpenChange).not.toHaveBeenCalled();

		resolveTask({ display_name: "Download slow.bin" });
		await waitFor(() => {
			expect(onOpenChange).toHaveBeenCalledWith(false);
		});
	});

	it("rejects non-http urls before submission", async () => {
		render(
			<OfflineDownloadDialog
				open
				onOpenChange={vi.fn()}
				targetFolderId={null}
				targetFolderName={null}
			/>,
		);

		fireEvent.change(
			screen.getByLabelText("tasks:offline_download_url_label"),
			{
				target: { value: "javascript:alert(1)" },
			},
		);
		fireEvent.click(
			screen.getByRole("button", {
				name: /tasks:offline_download_submit/,
			}),
		);

		await waitFor(() => {
			expect(mockState.createOfflineDownloadTask).not.toHaveBeenCalled();
		});
		expect(mockState.toastSuccess).not.toHaveBeenCalled();
		expect(mockState.toastError).toHaveBeenCalledWith(
			"tasks:offline_download_url_invalid",
		);
	});
	it("suppresses an old submission failure after reopening", async () => {
		const pending = deferred<never>();
		mockState.createOfflineDownloadTask.mockReturnValueOnce(pending.promise);
		const view = (open: boolean) => (
			<OfflineDownloadDialog
				open={open}
				onOpenChange={vi.fn()}
				targetFolderId={null}
			/>
		);
		const { rerender } = render(view(true));
		fireEvent.change(
			screen.getByLabelText("tasks:offline_download_url_label"),
			{ target: { value: "https://example.com/file.zip" } },
		);
		fireEvent.click(
			screen.getByRole("button", { name: /tasks:offline_download_submit/ }),
		);
		rerender(view(false));
		rerender(view(true));
		await act(async () => {
			pending.reject(new Error("old submit failure"));
			await pending.promise.catch(() => {});
		});
		expect(mockState.handleApiError).not.toHaveBeenCalled();
		expect(
			screen.getByLabelText("tasks:offline_download_url_label"),
		).toHaveValue("");
		fireEvent.change(
			screen.getByLabelText("tasks:offline_download_url_label"),
			{ target: { value: "https://example.com/new.zip" } },
		);
		expect(
			screen.getByRole("button", { name: /tasks:offline_download_submit/ }),
		).toBeEnabled();
	});
});
