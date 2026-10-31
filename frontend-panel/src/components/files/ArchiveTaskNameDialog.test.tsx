import {
	act,
	fireEvent,
	render,
	screen,
	waitFor,
} from "@testing-library/react";
import { beforeEach, describe, expect, it, vi } from "vitest";
import { ArchiveTaskNameDialog } from "@/components/files/ArchiveTaskNameDialog";
import { deferred } from "@/test/deferred";

const mockState = vi.hoisted(() => ({
	handleApiError: vi.fn(),
	onOpenChange: vi.fn(),
	onSubmit: vi.fn(),
}));

vi.mock("react-i18next", () => ({
	useTranslation: () => ({
		t: (key: string) => key,
	}),
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
	Dialog: ({ children, open }: { children: React.ReactNode; open: boolean }) =>
		open ? <div>{children}</div> : null,
	DialogContent: ({ children }: { children: React.ReactNode }) => (
		<div>{children}</div>
	),
	DialogHeader: ({ children }: { children: React.ReactNode }) => (
		<div>{children}</div>
	),
	DialogTitle: ({ children }: { children: React.ReactNode }) => (
		<div>{children}</div>
	),
}));

vi.mock("@/components/ui/input", () => ({
	Input: (props: React.InputHTMLAttributes<HTMLInputElement>) => {
		const { autoFocus: _autoFocus, ...restProps } = props;
		return <input {...restProps} />;
	},
}));

vi.mock("@/components/ui/label", () => ({
	Label: (props: React.LabelHTMLAttributes<HTMLLabelElement>) => (
		<span {...props} />
	),
}));

vi.mock("@/components/ui/select", () => ({
	Select: ({
		children,
		disabled,
		onValueChange,
		value,
	}: {
		children: React.ReactNode;
		disabled?: boolean;
		onValueChange?: (value: string) => void;
		value?: string;
	}) => (
		<div data-disabled={disabled} data-value={value}>
			{children}
			<button type="button" onClick={() => onValueChange?.("gb18030")}>
				select-gb18030
			</button>
		</div>
	),
	SelectContent: ({ children }: { children: React.ReactNode }) => (
		<div>{children}</div>
	),
	SelectItem: ({
		children,
		value,
	}: {
		children: React.ReactNode;
		value: string;
	}) => <div data-value={value}>{children}</div>,
	SelectTrigger: ({
		children,
		id,
	}: {
		children: React.ReactNode;
		id?: string;
	}) => (
		<button type="button" id={id}>
			{children}
		</button>
	),
	SelectValue: () => <span>select-value</span>,
}));

vi.mock("@/hooks/useApiError", () => ({
	handleApiError: (...args: unknown[]) => mockState.handleApiError(...args),
}));

describe("ArchiveTaskNameDialog", () => {
	beforeEach(() => {
		mockState.handleApiError.mockReset();
		mockState.onOpenChange.mockReset();
		mockState.onSubmit.mockReset();
		mockState.onSubmit.mockResolvedValue(undefined);
	});

	it("submits a trimmed custom archive name and closes on success", async () => {
		render(
			<ArchiveTaskNameDialog
				open
				onOpenChange={mockState.onOpenChange}
				mode="compress"
				initialName="notes.txt.zip"
				onSubmit={mockState.onSubmit}
			/>,
		);

		fireEvent.change(
			screen.getByPlaceholderText("tasks:archive_compress_name_placeholder"),
			{
				target: { value: " custom-bundle.zip " },
			},
		);
		fireEvent.click(screen.getByText("tasks:archive_compress_submit"));

		await waitFor(() => {
			expect(mockState.onSubmit).toHaveBeenCalledWith("custom-bundle.zip");
		});
		expect(mockState.onOpenChange).toHaveBeenCalledWith(false);
	});

	it("passes undefined when the name is cleared", async () => {
		render(
			<ArchiveTaskNameDialog
				open
				onOpenChange={mockState.onOpenChange}
				mode="extract"
				initialName="bundle"
				onSubmit={mockState.onSubmit}
			/>,
		);

		fireEvent.change(
			screen.getByPlaceholderText("tasks:archive_extract_name_placeholder"),
			{
				target: { value: "   " },
			},
		);
		fireEvent.click(screen.getByText("tasks:archive_extract_submit"));

		await waitFor(() => {
			expect(mockState.onSubmit).toHaveBeenCalledWith(undefined, "auto");
		});
		expect(mockState.onOpenChange).toHaveBeenCalledWith(false);
	});

	it("submits the selected filename encoding for extract tasks", async () => {
		render(
			<ArchiveTaskNameDialog
				open
				onOpenChange={mockState.onOpenChange}
				mode="extract"
				initialName="bundle"
				onSubmit={mockState.onSubmit}
			/>,
		);

		fireEvent.click(screen.getByText("select-gb18030"));
		fireEvent.click(screen.getByText("tasks:archive_extract_submit"));

		await waitFor(() => {
			expect(mockState.onSubmit).toHaveBeenCalledWith("bundle", "gb18030");
		});
		expect(mockState.onOpenChange).toHaveBeenCalledWith(false);
	});

	it("routes submit failures through handleApiError and keeps the dialog open", async () => {
		const error = new Error("submit failed");
		mockState.onSubmit.mockRejectedValueOnce(error);

		render(
			<ArchiveTaskNameDialog
				open
				onOpenChange={mockState.onOpenChange}
				mode="compress"
				initialName="notes.txt.zip"
				onSubmit={mockState.onSubmit}
			/>,
		);

		fireEvent.click(screen.getByText("tasks:archive_compress_submit"));

		await waitFor(() => {
			expect(mockState.handleApiError).toHaveBeenCalledWith(error);
		});
		expect(mockState.onOpenChange).not.toHaveBeenCalled();
	});
	it("does not report a submission error to a replacement archive session", async () => {
		const pending = deferred<void>();
		mockState.onSubmit.mockReturnValueOnce(pending.promise);
		const view = (open: boolean) => (
			<ArchiveTaskNameDialog
				open={open}
				onOpenChange={mockState.onOpenChange}
				onSubmit={mockState.onSubmit}
				mode="compress"
				initialName="archive"
			/>
		);
		const { rerender } = render(view(true));
		fireEvent.click(screen.getByText("tasks:archive_compress_submit"));
		rerender(view(false));
		rerender(view(true));
		await act(async () => {
			pending.reject(new Error("old archive failure"));
			await pending.promise.catch(() => {});
		});
		expect(mockState.handleApiError).not.toHaveBeenCalled();
		expect(mockState.onOpenChange).not.toHaveBeenCalled();
	});
});
