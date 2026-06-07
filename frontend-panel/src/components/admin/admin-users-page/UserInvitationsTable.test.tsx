import { fireEvent, render, screen } from "@testing-library/react";
import { describe, expect, it, vi } from "vitest";
import { UserInvitationsTable } from "@/components/admin/admin-users-page/UserInvitationsTable";
import type { AdminUserInvitationInfo } from "@/types/api";

vi.mock("react-i18next", () => ({
	useTranslation: () => ({
		t: (key: string, values?: Record<string, unknown>) =>
			values?.id ? `${key}:${values.id}` : key,
	}),
}));

vi.mock("@/components/common/AdminTable", () => ({
	ADMIN_TABLE_BADGE_CELL_CLASS: "badge-cell",
	ADMIN_TABLE_MONO_TEXT_CLASS: "mono-cell",
	ADMIN_TABLE_MUTED_TEXT_CLASS: "muted-cell",
	ADMIN_TABLE_STACKED_CELL_CLASS: "stacked-cell",
	ADMIN_TABLE_TEXT_CELL_CLASS: "text-cell",
	AdminTable: ({ children }: { children: React.ReactNode }) => (
		<table>{children}</table>
	),
	AdminTableBody: ({ children }: { children: React.ReactNode }) => (
		<tbody>{children}</tbody>
	),
	AdminTableCell: ({
		children,
		className,
	}: {
		children: React.ReactNode;
		className?: string;
	}) => <td className={className}>{children}</td>,
	AdminTableHead: ({
		children,
		className,
	}: {
		children: React.ReactNode;
		className?: string;
	}) => <th className={className}>{children}</th>,
	AdminTableHeader: ({ children }: { children: React.ReactNode }) => (
		<thead>{children}</thead>
	),
	AdminTableRow: ({ children }: { children: React.ReactNode }) => (
		<tr>{children}</tr>
	),
	AdminTableShell: ({ children }: { children: React.ReactNode }) => (
		<div>{children}</div>
	),
}));

vi.mock("@/components/ui/badge", () => ({
	Badge: ({
		children,
		className,
	}: {
		children: React.ReactNode;
		className?: string;
	}) => <span className={className}>{children}</span>,
}));

vi.mock("@/components/ui/button", () => ({
	Button: ({
		children,
		disabled,
		onClick,
		type,
		...props
	}: {
		children?: React.ReactNode;
		disabled?: boolean;
		onClick?: () => void;
		type?: "button" | "submit";
		[key: string]: unknown;
	}) => (
		<button
			type={type ?? "button"}
			disabled={disabled}
			onClick={onClick}
			{...props}
		>
			{children}
		</button>
	),
}));

vi.mock("@/components/ui/icon", () => ({
	Icon: ({ name }: { name: string }) => (
		<span aria-hidden="true" data-icon-name={name} />
	),
}));

vi.mock("@/lib/format", () => ({
	formatDateAbsolute: (value: string) => `date:${value}`,
	formatDateAbsoluteWithOffset: (value: string) => `offset:${value}`,
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
		id: 1,
		invitation_url: "https://drive.example.test/invite/token",
		invited_by: 1,
		mail_queued: false,
		revoked_at: null,
		status: "pending",
		updated_at: "2026-06-07T10:00:00Z",
		...overrides,
	};
}

function renderTable(
	props: Partial<React.ComponentProps<typeof UserInvitationsTable>> = {},
) {
	const defaultProps: React.ComponentProps<typeof UserInvitationsTable> = {
		invitations: [invitation()],
		onRevokeInvitation: vi.fn(),
		revokingInvitationId: null,
	};

	return {
		...render(<UserInvitationsTable {...defaultProps} {...props} />),
		props: { ...defaultProps, ...props },
	};
}

describe("UserInvitationsTable", () => {
	it("renders invitation status, dates, accepted user, and enabled pending actions", () => {
		const onRevokeInvitation = vi.fn();
		const item = invitation({
			accepted_user_id: 44,
		});

		renderTable({
			invitations: [item],
			onRevokeInvitation,
		});

		expect(screen.getByText("invitee@example.com")).toBeInTheDocument();
		expect(screen.getByText("invitation_status_pending")).toBeInTheDocument();
		expect(screen.getByText("invitation_accepted_user:44")).toBeInTheDocument();
		expect(screen.getByText("date:2026-06-10T10:00:00Z")).toBeInTheDocument();
		expect(screen.getByText("date:2026-06-07T10:00:00Z")).toBeInTheDocument();
		expect(
			screen.queryByRole("button", { name: "invitation_copy_link" }),
		).not.toBeInTheDocument();

		fireEvent.click(screen.getByRole("button", { name: "revoke_invitation" }));
		expect(onRevokeInvitation).toHaveBeenCalledWith(item);
	});

	it("disables revoke for non-pending invitations", () => {
		const onRevokeInvitation = vi.fn();

		renderTable({
			invitations: [
				invitation({
					id: 2,
					invitation_url: null,
					status: "accepted",
				}),
			],
			onRevokeInvitation,
		});

		expect(screen.getByText("invitation_status_accepted")).toBeInTheDocument();
		expect(
			screen.getByRole("button", { name: "revoke_invitation" }),
		).toBeDisabled();
	});

	it("shows a spinner and blocks revoke while the invitation is pending revoke", () => {
		const { container } = renderTable({
			revokingInvitationId: 1,
		});

		expect(
			screen.getByRole("button", { name: "revoke_invitation" }),
		).toBeDisabled();
		expect(
			container.querySelector('[data-icon-name="Spinner"]'),
		).toBeInTheDocument();
	});
});
