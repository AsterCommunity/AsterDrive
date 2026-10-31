import {
	act,
	fireEvent,
	render,
	screen,
	waitFor,
} from "@testing-library/react";
import { beforeEach, describe, expect, it, vi } from "vitest";
import { CreateFileDialog } from "@/components/files/CreateFileDialog";
import { CreateFolderDialog } from "@/components/files/CreateFolderDialog";
import { deferred } from "@/test/deferred";

const mockState = vi.hoisted(() => ({
	createFile: vi.fn(),
	createFolder: vi.fn(),
	handleApiError: vi.fn(),
	toastSuccess: vi.fn(),
}));

vi.mock("react-i18next", () => ({
	useTranslation: () => ({
		t: (key: string) => key,
	}),
}));

vi.mock("sonner", () => ({
	toast: {
		success: (...args: unknown[]) => mockState.toastSuccess(...args),
	},
}));

vi.mock("@/components/ui/button", () => ({
	Button: ({
		children,
		type,
		disabled,
		onClick,
		className,
	}: {
		children: React.ReactNode;
		type?: "button" | "submit";
		disabled?: boolean;
		onClick?: () => void;
		className?: string;
	}) => (
		<button
			type={type ?? "button"}
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
	DialogContent: ({ children }: { children: React.ReactNode }) => (
		<div>{children}</div>
	),
	DialogHeader: ({ children }: { children: React.ReactNode }) => (
		<div>{children}</div>
	),
	DialogTitle: ({ children }: { children: React.ReactNode }) => (
		<h2>{children}</h2>
	),
}));

vi.mock("@/components/ui/input", () => ({
	Input: ({ ...props }: React.InputHTMLAttributes<HTMLInputElement>) => (
		<input {...props} />
	),
}));

vi.mock("@/hooks/useApiError", () => ({
	handleApiError: (...args: unknown[]) => mockState.handleApiError(...args),
}));

vi.mock("@/stores/fileStore", () => ({
	useFileStore: (
		selector: (state: {
			createFile: typeof mockState.createFile;
			createFolder: typeof mockState.createFolder;
		}) => unknown,
	) =>
		selector({
			createFile: mockState.createFile,
			createFolder: mockState.createFolder,
		}),
}));

describe("Create dialogs", () => {
	beforeEach(() => {
		mockState.createFile.mockReset();
		mockState.createFolder.mockReset();
		mockState.handleApiError.mockReset();
		mockState.toastSuccess.mockReset();
		mockState.createFile.mockResolvedValue(undefined);
		mockState.createFolder.mockResolvedValue(undefined);
	});

	it("ignores blank file names", () => {
		const onOpenChange = vi.fn();

		render(<CreateFileDialog open onOpenChange={onOpenChange} />);

		fireEvent.change(screen.getByPlaceholderText("file_name"), {
			target: { value: "   " },
		});
		fireEvent.click(screen.getByRole("button", { name: "create_file" }));

		expect(mockState.createFile).not.toHaveBeenCalled();
		expect(mockState.toastSuccess).not.toHaveBeenCalled();
		expect(onOpenChange).not.toHaveBeenCalled();
	});

	it("creates files with trimmed names and retains the field during exit", async () => {
		const onOpenChange = vi.fn();

		render(<CreateFileDialog open onOpenChange={onOpenChange} />);

		const input = screen.getByPlaceholderText("file_name");

		fireEvent.change(input, {
			target: { value: "  notes.md  " },
		});
		fireEvent.click(screen.getByRole("button", { name: "create_file" }));

		await waitFor(() => {
			expect(mockState.createFile).toHaveBeenCalledWith("notes.md");
			expect(mockState.toastSuccess).toHaveBeenCalledWith(
				"create_file_success",
			);
			expect(onOpenChange).toHaveBeenCalledWith(false);
			expect(input).toHaveValue("  notes.md  ");
		});
	});

	it("ignores duplicate file submits while creation is pending", async () => {
		const onOpenChange = vi.fn();
		let resolveCreate: (() => void) | undefined;
		mockState.createFile.mockReturnValueOnce(
			new Promise<void>((resolve) => {
				resolveCreate = resolve;
			}),
		);

		render(<CreateFileDialog open onOpenChange={onOpenChange} />);

		fireEvent.change(screen.getByPlaceholderText("file_name"), {
			target: { value: "notes.md" },
		});
		const button = screen.getByRole("button", { name: "create_file" });

		fireEvent.click(button);
		fireEvent.click(button);

		expect(mockState.createFile).toHaveBeenCalledTimes(1);
		expect(button).toBeDisabled();

		resolveCreate?.();

		await waitFor(() => {
			expect(mockState.toastSuccess).toHaveBeenCalledWith(
				"create_file_success",
			);
		});
	});

	it("reports file creation failures and keeps the dialog open", async () => {
		const onOpenChange = vi.fn();
		const error = new Error("cannot create file");
		mockState.createFile.mockRejectedValueOnce(error);

		render(<CreateFileDialog open onOpenChange={onOpenChange} />);

		fireEvent.change(screen.getByPlaceholderText("file_name"), {
			target: { value: "draft.md" },
		});
		fireEvent.click(screen.getByRole("button", { name: "create_file" }));

		await waitFor(() => {
			expect(mockState.handleApiError).toHaveBeenCalledWith(error);
		});
		expect(mockState.toastSuccess).not.toHaveBeenCalled();
		expect(onOpenChange).not.toHaveBeenCalled();
		expect(screen.getByPlaceholderText("file_name")).toHaveValue("draft.md");
	});

	it("ignores blank folder names", () => {
		const onOpenChange = vi.fn();

		render(<CreateFolderDialog open onOpenChange={onOpenChange} />);

		fireEvent.change(screen.getByPlaceholderText("folder_name"), {
			target: { value: "   " },
		});
		fireEvent.click(screen.getByRole("button", { name: "create_folder" }));

		expect(mockState.createFolder).not.toHaveBeenCalled();
		expect(mockState.toastSuccess).not.toHaveBeenCalled();
		expect(onOpenChange).not.toHaveBeenCalled();
	});

	it("creates folders with trimmed names and retains the field during exit", async () => {
		const onOpenChange = vi.fn();

		render(<CreateFolderDialog open onOpenChange={onOpenChange} />);

		const input = screen.getByPlaceholderText("folder_name");

		fireEvent.change(input, {
			target: { value: "  Projects  " },
		});
		fireEvent.click(screen.getByRole("button", { name: "create_folder" }));

		await waitFor(() => {
			expect(mockState.createFolder).toHaveBeenCalledWith("Projects");
			expect(mockState.toastSuccess).toHaveBeenCalledWith(
				"create_folder_success",
			);
			expect(onOpenChange).toHaveBeenCalledWith(false);
			expect(input).toHaveValue("  Projects  ");
		});
	});

	it("ignores duplicate folder submits while creation is pending", async () => {
		const onOpenChange = vi.fn();
		let resolveCreate: (() => void) | undefined;
		mockState.createFolder.mockReturnValueOnce(
			new Promise<void>((resolve) => {
				resolveCreate = resolve;
			}),
		);

		render(<CreateFolderDialog open onOpenChange={onOpenChange} />);

		fireEvent.change(screen.getByPlaceholderText("folder_name"), {
			target: { value: "Projects" },
		});
		const button = screen.getByRole("button", { name: "create_folder" });

		fireEvent.click(button);
		fireEvent.click(button);

		expect(mockState.createFolder).toHaveBeenCalledTimes(1);
		expect(button).toBeDisabled();

		resolveCreate?.();

		await waitFor(() => {
			expect(mockState.toastSuccess).toHaveBeenCalledWith(
				"create_folder_success",
			);
		});
	});

	it("reports folder creation failures and keeps the dialog open", async () => {
		const onOpenChange = vi.fn();
		const error = new Error("cannot create folder");
		mockState.createFolder.mockRejectedValueOnce(error);

		render(<CreateFolderDialog open onOpenChange={onOpenChange} />);

		fireEvent.change(screen.getByPlaceholderText("folder_name"), {
			target: { value: "Archive" },
		});
		fireEvent.click(screen.getByRole("button", { name: "create_folder" }));

		await waitFor(() => {
			expect(mockState.handleApiError).toHaveBeenCalledWith(error);
		});
		expect(mockState.toastSuccess).not.toHaveBeenCalled();
		expect(onOpenChange).not.toHaveBeenCalled();
		expect(screen.getByPlaceholderText("folder_name")).toHaveValue("Archive");
	});
	it.each(["file", "folder"] as const)(
		"ignores an old %s creation error after reopening",
		async (kind) => {
			const pending = deferred<void>();
			mockState[
				kind === "file" ? "createFile" : "createFolder"
			].mockReturnValueOnce(pending.promise);
			const Component = kind === "file" ? CreateFileDialog : CreateFolderDialog;
			const view = (open: boolean) => (
				<Component open={open} onOpenChange={vi.fn()} />
			);
			const { rerender } = render(view(true));
			fireEvent.change(screen.getByPlaceholderText(`${kind}_name`), {
				target: { value: "draft" },
			});
			fireEvent.click(screen.getByRole("button", { name: `create_${kind}` }));
			rerender(view(false));
			rerender(view(true));
			await act(async () => {
				pending.reject(new Error("old failure"));
				await pending.promise.catch(() => {});
			});
			expect(mockState.handleApiError).not.toHaveBeenCalled();
			expect(screen.getByPlaceholderText(`${kind}_name`)).toHaveValue("");
			expect(
				screen.getByRole("button", { name: `create_${kind}` }),
			).toBeEnabled();
		},
	);
});
