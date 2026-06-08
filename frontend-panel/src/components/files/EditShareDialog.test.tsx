import { fireEvent, render, screen, waitFor } from "@testing-library/react";
import { beforeEach, describe, expect, it, vi } from "vitest";
import { EditShareDialog } from "@/components/files/EditShareDialog";
import type { MyShareInfo } from "@/types/api";

const mockState = vi.hoisted(() => ({
	handleApiError: vi.fn(),
	toastError: vi.fn(),
	toastSuccess: vi.fn(),
	update: vi.fn(),
}));

vi.mock("react-i18next", () => ({
	useTranslation: () => ({
		t: (key: string, opts?: Record<string, unknown>) => {
			if (key === "share:my_shares_edit_title") {
				return `${key}:${opts?.name}`;
			}
			return key;
		},
	}),
}));

vi.mock("sonner", () => ({
	toast: {
		error: (...args: unknown[]) => mockState.toastError(...args),
		success: (...args: unknown[]) => mockState.toastSuccess(...args),
	},
}));

vi.mock("@/hooks/useApiError", () => ({
	handleApiError: (...args: unknown[]) => mockState.handleApiError(...args),
}));

vi.mock("@/services/shareService", () => ({
	shareService: {
		update: (...args: unknown[]) => mockState.update(...args),
	},
}));

vi.mock("@/components/ui/button", () => ({
	Button: ({
		children,
		type,
		disabled,
		form,
		onClick,
	}: {
		children: React.ReactNode;
		type?: "button" | "submit";
		disabled?: boolean;
		form?: string;
		onClick?: () => void;
	}) => (
		<button
			type={type ?? "button"}
			disabled={disabled}
			form={form}
			onClick={onClick}
		>
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
	DialogDescription: ({ children }: { children: React.ReactNode }) => (
		<p>{children}</p>
	),
	DialogHeader: ({ children }: { children: React.ReactNode }) => (
		<div>{children}</div>
	),
	DialogTitle: ({ children }: { children: React.ReactNode }) => (
		<h2>{children}</h2>
	),
}));

vi.mock("@/components/ui/icon", () => ({
	Icon: ({ name }: { name: string }) => <span>{name}</span>,
}));

vi.mock("@/components/ui/input", () => ({
	Input: (props: React.InputHTMLAttributes<HTMLInputElement>) => (
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

vi.mock("@/components/ui/select", async () => {
	const React = await import("react");

	function collectOptions(children: React.ReactNode): Array<{
		label: React.ReactNode;
		value: string;
	}> {
		const options: Array<{ label: React.ReactNode; value: string }> = [];

		React.Children.forEach(children, (child) => {
			if (!React.isValidElement(child)) return;

			if (typeof child.props.value === "string") {
				options.push({ label: child.props.children, value: child.props.value });
			}

			if (child.props.children) {
				options.push(...collectOptions(child.props.children));
			}
		});

		return options;
	}

	return {
		Select: ({
			children,
			value,
			onValueChange,
		}: {
			children: React.ReactNode;
			value?: string;
			onValueChange?: (value: string) => void;
		}) => (
			<select
				data-testid="share-password-action"
				value={value}
				onChange={(event) => onValueChange?.(event.target.value)}
			>
				{collectOptions(children).map((option) => (
					<option key={option.value} value={option.value}>
						{option.label}
					</option>
				))}
			</select>
		),
		SelectTrigger: ({ children }: { children?: React.ReactNode }) => (
			<>{children}</>
		),
		SelectValue: () => null,
		SelectContent: ({ children }: { children?: React.ReactNode }) => (
			<>{children}</>
		),
		SelectItem: ({
			children,
			value,
		}: {
			children: React.ReactNode;
			value: string;
		}) => <option value={value}>{children}</option>,
	};
});

function createShare(overrides: Partial<MyShareInfo> = {}): MyShareInfo {
	return {
		id: 7,
		token: "share-token",
		resource_id: 99,
		resource_name: "Document.pdf",
		resource_type: "file",
		resource_deleted: false,
		has_password: true,
		status: "active",
		expires_at: "2026-04-02T12:00:00Z",
		max_downloads: 5,
		download_count: 0,
		view_count: 0,
		remaining_downloads: 5,
		created_at: "2026-03-28T00:00:00Z",
		updated_at: "2026-03-28T00:00:00Z",
		...overrides,
	};
}

describe("EditShareDialog", () => {
	beforeEach(() => {
		mockState.handleApiError.mockReset();
		mockState.toastError.mockReset();
		mockState.toastSuccess.mockReset();
		mockState.update.mockReset();
		mockState.update.mockResolvedValue({ id: 7 });
	});

	it("updates share settings and calls onSaved", async () => {
		const onSaved = vi.fn();

		render(
			<EditShareDialog
				open
				onOpenChange={vi.fn()}
				share={createShare()}
				onSaved={onSaved}
			/>,
		);

		fireEvent.change(screen.getByTestId("share-password-action"), {
			target: { value: "set" },
		});
		const passwordInput = screen.getByLabelText(
			"share:share_password_optional",
		);
		expect(passwordInput).toHaveAttribute("autocomplete", "new-password");
		fireEvent.change(passwordInput, {
			target: { value: "new-secret" },
		});
		fireEvent.change(screen.getByLabelText("share:share_expiration"), {
			target: { value: "2026-04-03T08:30" },
		});
		fireEvent.change(screen.getByLabelText("share:share_download_limit"), {
			target: { value: "8" },
		});
		fireEvent.click(screen.getByRole("button", { name: "core:save" }));

		await waitFor(() => {
			expect(mockState.update).toHaveBeenCalledWith(7, {
				password: "new-secret",
				expires_at: new Date("2026-04-03T08:30").toISOString(),
				max_downloads: 8,
			});
		});
		expect(mockState.toastSuccess).toHaveBeenCalledWith(
			"share:my_shares_edit_success",
		);
		expect(onSaved).toHaveBeenCalled();
	});

	it("allows clearing the password without sending a replacement", async () => {
		render(
			<EditShareDialog open onOpenChange={vi.fn()} share={createShare()} />,
		);

		fireEvent.change(screen.getByTestId("share-password-action"), {
			target: { value: "clear" },
		});
		fireEvent.change(screen.getByLabelText("share:share_expiration"), {
			target: { value: "" },
		});
		fireEvent.change(screen.getByLabelText("share:share_download_limit"), {
			target: { value: "0" },
		});
		fireEvent.click(screen.getByRole("button", { name: "core:save" }));

		await waitFor(() => {
			expect(mockState.update).toHaveBeenCalledWith(7, {
				password: "",
				expires_at: null,
				max_downloads: 0,
			});
		});
	});

	it("guards duplicate save submits synchronously", async () => {
		let resolveUpdate: (() => void) | null = null;
		mockState.update.mockImplementationOnce(
			() =>
				new Promise((resolve) => {
					resolveUpdate = () => resolve({ id: 7 });
				}),
		);

		render(
			<EditShareDialog open onOpenChange={vi.fn()} share={createShare()} />,
		);

		const saveButton = screen.getByRole("button", { name: "core:save" });
		fireEvent.click(saveButton);
		fireEvent.click(saveButton);

		await waitFor(() => {
			expect(mockState.update).toHaveBeenCalledTimes(1);
		});

		resolveUpdate?.();
		await waitFor(() => {
			expect(mockState.toastSuccess).toHaveBeenCalledWith(
				"share:my_shares_edit_success",
			);
		});
	});
});
