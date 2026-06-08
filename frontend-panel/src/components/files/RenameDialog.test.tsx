import { fireEvent, render, screen, waitFor } from "@testing-library/react";
import { beforeEach, describe, expect, it, vi } from "vitest";
import { RenameDialog } from "@/components/files/RenameDialog";

const mockState = vi.hoisted(() => ({
	handleApiError: vi.fn(),
	refresh: vi.fn(),
	renameFile: vi.fn(),
	renameFolder: vi.fn(),
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
		onClick,
		className,
	}: {
		children: React.ReactNode;
		type?: "button" | "submit";
		onClick?: () => void;
		className?: string;
	}) => (
		<button type={type ?? "button"} onClick={onClick} className={className}>
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

vi.mock("@/services/fileService", () => ({
	fileService: {
		renameFile: (...args: unknown[]) => mockState.renameFile(...args),
		renameFolder: (...args: unknown[]) => mockState.renameFolder(...args),
	},
}));

vi.mock("@/stores/fileStore", () => ({
	useFileStore: (
		selector: (state: { refresh: typeof mockState.refresh }) => unknown,
	) => selector({ refresh: mockState.refresh }),
}));

describe("RenameDialog", () => {
	beforeEach(() => {
		mockState.handleApiError.mockReset();
		mockState.refresh.mockReset();
		mockState.renameFile.mockReset();
		mockState.renameFolder.mockReset();
		mockState.toastSuccess.mockReset();
		mockState.refresh.mockResolvedValue(undefined);
		mockState.renameFile.mockResolvedValue(undefined);
		mockState.renameFolder.mockResolvedValue(undefined);
	});

	it("closes immediately for blank trimmed names without calling rename services", () => {
		const onOpenChange = vi.fn();

		render(
			<RenameDialog
				open
				onOpenChange={onOpenChange}
				type="file"
				id={1}
				currentName="notes.md"
			/>,
		);

		fireEvent.change(screen.getByDisplayValue("notes.md"), {
			target: { value: "   " },
		});
		fireEvent.click(screen.getByRole("button", { name: "rename" }));

		expect(onOpenChange).toHaveBeenCalledWith(false);
		expect(mockState.renameFile).not.toHaveBeenCalled();
		expect(mockState.renameFolder).not.toHaveBeenCalled();
		expect(mockState.refresh).not.toHaveBeenCalled();
	});

	it("closes when the trimmed name matches the current name", () => {
		const onOpenChange = vi.fn();

		render(
			<RenameDialog
				open
				onOpenChange={onOpenChange}
				type="folder"
				id={2}
				currentName="Projects"
			/>,
		);

		fireEvent.change(screen.getByDisplayValue("Projects"), {
			target: { value: "  Projects  " },
		});
		fireEvent.click(screen.getByRole("button", { name: "rename" }));

		expect(onOpenChange).toHaveBeenCalledWith(false);
		expect(mockState.renameFolder).not.toHaveBeenCalled();
		expect(mockState.refresh).not.toHaveBeenCalled();
	});

	it("renames files, refreshes the store, and selects the filename stem on focus", async () => {
		const onOpenChange = vi.fn();

		render(
			<RenameDialog
				open
				onOpenChange={onOpenChange}
				type="file"
				id={7}
				currentName="draft.md"
			/>,
		);

		const input = screen.getByDisplayValue("draft.md");
		const setSelectionRange = vi.spyOn(input, "setSelectionRange");
		const select = vi.spyOn(input, "select");

		fireEvent.focus(input);
		expect(setSelectionRange).toHaveBeenCalledWith(0, 5);
		expect(select).not.toHaveBeenCalled();

		fireEvent.change(input, { target: { value: "  draft-final.md  " } });
		fireEvent.click(screen.getByRole("button", { name: "rename" }));

		await waitFor(() => {
			expect(mockState.renameFile).toHaveBeenCalledWith(7, "draft-final.md");
		});
		expect(mockState.toastSuccess).toHaveBeenCalledWith("rename_success");
		expect(onOpenChange).toHaveBeenCalledWith(false);
		expect(mockState.refresh).toHaveBeenCalledTimes(1);
	});

	it("uses the provided rename callback instead of refreshing the store", async () => {
		const onOpenChange = vi.fn();
		const onRenamed = vi.fn().mockResolvedValue(undefined);

		render(
			<RenameDialog
				open
				onOpenChange={onOpenChange}
				type="file"
				id={7}
				currentName="draft.md"
				onRenamed={onRenamed}
			/>,
		);

		fireEvent.change(screen.getByDisplayValue("draft.md"), {
			target: { value: "draft-final.md" },
		});
		fireEvent.click(screen.getByRole("button", { name: "rename" }));

		await waitFor(() => {
			expect(mockState.renameFile).toHaveBeenCalledWith(7, "draft-final.md");
		});
		expect(onOpenChange).toHaveBeenCalledWith(false);
		expect(onRenamed).toHaveBeenCalledTimes(1);
		expect(mockState.refresh).not.toHaveBeenCalled();
	});

	it("renames folders and reports service failures without closing the dialog", async () => {
		const onOpenChange = vi.fn();
		const error = new Error("rename failed");
		mockState.renameFolder.mockRejectedValueOnce(error);

		render(
			<RenameDialog
				open
				onOpenChange={onOpenChange}
				type="folder"
				id={9}
				currentName="Archive"
			/>,
		);

		const input = screen.getByDisplayValue("Archive");
		const select = vi.spyOn(input, "select");

		fireEvent.focus(input);
		expect(select).toHaveBeenCalledTimes(1);

		fireEvent.change(input, { target: { value: "Archives 2026" } });
		fireEvent.click(screen.getByRole("button", { name: "rename" }));

		await waitFor(() => {
			expect(mockState.renameFolder).toHaveBeenCalledWith(9, "Archives 2026");
		});
		expect(mockState.handleApiError).toHaveBeenCalledWith(error);
		expect(mockState.toastSuccess).not.toHaveBeenCalled();
		expect(onOpenChange).not.toHaveBeenCalledWith(false);
		expect(mockState.refresh).not.toHaveBeenCalled();
	});
});
