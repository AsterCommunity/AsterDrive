import { fireEvent, render, screen } from "@testing-library/react";
import { describe, expect, it, vi } from "vitest";
import { InviteUserDialog } from "@/components/admin/admin-users-page/InviteUserDialog";
import type { AdminUserInvitationInfo } from "@/types/api";

vi.mock("react-i18next", () => ({
	useTranslation: () => ({
		t: (key: string) => key.replace(/^core:/, ""),
	}),
}));

vi.mock("@/components/ui/button", () => ({
	Button: ({
		"aria-label": ariaLabel,
		children,
		disabled,
		onClick,
		title,
		type,
	}: {
		"aria-label"?: string;
		children?: React.ReactNode;
		disabled?: boolean;
		onClick?: () => void;
		title?: string;
		type?: "button" | "submit";
	}) => (
		<button
			type={type ?? "button"}
			aria-label={ariaLabel}
			disabled={disabled}
			onClick={onClick}
			title={title}
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
	}) =>
		open ? (
			<div>
				<button type="button" onClick={() => onOpenChange(false)}>
					external-dialog-close
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
	Icon: ({ name }: { name: string }) => (
		<span
			aria-hidden="true"
			data-icon-name={name}
			data-testid={`icon-${name}`}
		/>
	),
}));

vi.mock("@/components/ui/input", () => ({
	Input: ({
		ariaInvalid,
		onChange,
		onFocus,
		...props
	}: React.InputHTMLAttributes<HTMLInputElement> & {
		ariaInvalid?: boolean;
	}) => (
		<input
			aria-invalid={ariaInvalid}
			onChange={onChange}
			onFocus={onFocus}
			{...props}
		/>
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

function invitation(
	overrides: Partial<AdminUserInvitationInfo> = {},
): AdminUserInvitationInfo {
	return {
		accepted_at: null,
		accepted_user_id: null,
		created_at: "2026-06-07T10:00:00Z",
		email: "invitee@example.com",
		expires_at: "2026-06-10T10:00:00Z",
		id: 101,
		invitation_url: " https://drive.example.test/invite/token ",
		invited_by: 1,
		mail_queued: true,
		revoked_at: null,
		status: "pending",
		updated_at: "2026-06-07T10:00:00Z",
		...overrides,
	};
}

function renderDialog(
	props: Partial<React.ComponentProps<typeof InviteUserDialog>> = {},
) {
	const defaultProps: React.ComponentProps<typeof InviteUserDialog> = {
		createdInvitation: null,
		errors: {},
		form: { email: "" },
		inviting: false,
		onCopyLink: vi.fn(),
		onFieldChange: vi.fn(),
		onFieldValidate: vi.fn(),
		onOpenChange: vi.fn(),
		onSubmit: vi.fn((event: React.FormEvent<HTMLFormElement>) =>
			event.preventDefault(),
		),
		open: true,
	};

	return {
		...render(<InviteUserDialog {...defaultProps} {...props} />),
		props: { ...defaultProps, ...props },
	};
}

describe("InviteUserDialog", () => {
	it("trims email validation input and reports form changes", () => {
		const onFieldChange = vi.fn();
		const onFieldValidate = vi.fn();

		renderDialog({
			form: { email: "draft@example.com" },
			onFieldChange,
			onFieldValidate,
		});

		fireEvent.change(screen.getByLabelText("email"), {
			target: { value: " invitee@example.com " },
		});

		expect(onFieldChange).toHaveBeenCalledWith("email", "invitee@example.com");
		expect(onFieldValidate).toHaveBeenCalledWith(
			"email",
			"invitee@example.com",
		);
	});

	it("shows validation errors and disables actions while inviting", () => {
		const onOpenChange = vi.fn();

		renderDialog({
			errors: { email: "email_format" },
			inviting: true,
			onOpenChange,
		});

		expect(screen.getByText("email_format")).toBeInTheDocument();
		const emailInput = screen.getByLabelText("email");
		const emailError = screen.getByText("email_format");
		expect(emailError).toHaveAttribute("id", "invite-user-email-error");
		expect(emailInput).toHaveAttribute("aria-describedby", emailError.id);
		expect(screen.getByLabelText("email")).toHaveAttribute(
			"aria-invalid",
			"true",
		);
		expect(screen.getByRole("button", { name: "cancel" })).toBeDisabled();
		expect(
			screen.getByRole("button", { name: /send_invitation/i }),
		).toBeDisabled();
		expect(screen.getByTestId("icon-Spinner")).toBeInTheDocument();

		fireEvent.click(
			screen.getByRole("button", { name: "external-dialog-close" }),
		);
		expect(onOpenChange).not.toHaveBeenCalled();
	});

	it("allows external close attempts when not inviting", () => {
		const onOpenChange = vi.fn();

		renderDialog({ onOpenChange });

		fireEvent.click(
			screen.getByRole("button", { name: "external-dialog-close" }),
		);

		expect(onOpenChange).toHaveBeenCalledWith(false);
	});

	it("renders created invitation metadata and copies the trimmed link", () => {
		const onCopyLink = vi.fn();

		renderDialog({
			createdInvitation: invitation(),
			form: { email: "invitee@example.com" },
			onCopyLink,
		});

		expect(screen.getByText("invitation_created")).toBeInTheDocument();
		expect(screen.getByText("invitation_mail_queued")).toBeInTheDocument();
		expect(
			screen.getByDisplayValue("https://drive.example.test/invite/token"),
		).toBeInTheDocument();

		fireEvent.click(
			screen.getByRole("button", { name: "invitation_copy_link" }),
		);

		expect(onCopyLink).toHaveBeenCalledWith(
			"https://drive.example.test/invite/token",
		);
	});

	it("hides the copy field when a created invitation has no usable URL", () => {
		renderDialog({
			createdInvitation: invitation({ invitation_url: "   " }),
			form: { email: "invitee@example.com" },
		});

		expect(screen.getByText("invitation_created")).toBeInTheDocument();
		expect(
			screen.queryByRole("button", { name: "invitation_copy_link" }),
		).not.toBeInTheDocument();
	});
});
