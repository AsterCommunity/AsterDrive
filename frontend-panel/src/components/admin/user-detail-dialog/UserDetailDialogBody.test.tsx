import { fireEvent, render, screen, waitFor } from "@testing-library/react";
import { beforeEach, describe, expect, it, vi } from "vitest";
import { UserDetailDialogBody } from "@/components/admin/user-detail-dialog/UserDetailDialogBody";
import type { StoragePolicyGroup, UserInfo } from "@/types/api";

vi.mock("react-i18next", () => ({
	useTranslation: () => ({
		t: (key: string) => key.replace(/^core:/, ""),
	}),
}));

vi.mock("sonner", () => ({
	toast: {
		success: vi.fn(),
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
	DialogFooter: ({ children }: { children: React.ReactNode }) => (
		<footer>{children}</footer>
	),
}));

vi.mock("@/components/ui/icon", () => ({
	Icon: ({ name }: { name: string }) => <span>{name}</span>,
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

vi.mock("@/components/ui/switch", () => ({
	Switch: ({
		"aria-label": ariaLabel,
		checked,
		disabled,
		onCheckedChange,
	}: {
		"aria-label"?: string;
		checked?: boolean;
		disabled?: boolean;
		onCheckedChange?: (value: boolean) => void;
	}) => (
		<button
			type="button"
			role="switch"
			aria-checked={checked}
			aria-label={ariaLabel}
			disabled={disabled}
			onClick={() => onCheckedChange?.(!checked)}
		/>
	),
}));

vi.mock("@/hooks/useApiError", () => ({
	handleApiError: vi.fn(),
}));

vi.mock("@/lib/validation", () => ({
	passwordSchema: {
		safeParse: (value: string) =>
			value.length >= 8
				? { success: true }
				: {
						success: false,
						error: { issues: [{ message: "password-short" }] },
					},
	},
}));

vi.mock("@/services/adminService", () => ({
	adminUserService: {
		resetMfa: vi.fn(),
		resetPassword: vi.fn(),
		revokeSessions: vi.fn(),
	},
}));

vi.mock("@/components/admin/user-detail-dialog/UserDetailSidebar", () => ({
	UserDetailSidebar: ({ user }: { user: UserInfo }) => (
		<aside>{user.username}</aside>
	),
}));

vi.mock("@/components/admin/user-detail-dialog/UserProfileSection", () => ({
	UserProfileSection: () => <section>profile-section</section>,
}));

vi.mock("@/components/admin/user-detail-dialog/UserPolicyGroupSection", () => ({
	UserPolicyGroupSection: () => <section>policy-section</section>,
}));

function user(overrides: Partial<UserInfo> = {}): UserInfo {
	return {
		created_at: "2026-03-28T00:00:00Z",
		email: "alice@example.com",
		email_verified: true,
		id: 11,
		must_change_password: false,
		pending_email: null,
		policy_group_id: null,
		profile: {
			avatar: {
				source: "none",
				url_512: null,
				url_1024: null,
				version: 0,
			},
			display_name: null,
		},
		role: "user",
		status: "active",
		storage_quota: 0,
		storage_used: 0,
		updated_at: "2026-03-28T00:00:00Z",
		username: "alice",
		...overrides,
	};
}

function renderDialog(overrides: Partial<UserInfo> = {}) {
	const onUpdate = vi.fn().mockResolvedValue(undefined);
	render(
		<UserDetailDialogBody
			onClose={vi.fn()}
			onRefreshPolicyGroups={vi.fn().mockResolvedValue(undefined)}
			onUpdate={onUpdate}
			policyGroups={[] satisfies StoragePolicyGroup[]}
			policyGroupsLoading={false}
			user={user(overrides)}
		/>,
	);
	return { onUpdate };
}

describe("UserDetailDialogBody forced password-change control", () => {
	beforeEach(() => {
		vi.clearAllMocks();
	});

	it("saves enabling the forced password-change flag through the user PATCH payload", async () => {
		const { onUpdate } = renderDialog();

		expect(
			screen.queryByRole("button", { name: "save_changes" }),
		).not.toBeInTheDocument();

		fireEvent.click(
			screen.getByRole("switch", { name: "force_password_change" }),
		);
		fireEvent.click(screen.getByRole("button", { name: "save_changes" }));

		await waitFor(() => {
			expect(onUpdate).toHaveBeenCalledWith(11, {
				must_change_password: true,
			});
		});
	});

	it("saves clearing an existing forced password-change flag", async () => {
		const { onUpdate } = renderDialog({ must_change_password: true });

		expect(
			screen.getByText("force_password_change_enabled"),
		).toBeInTheDocument();
		fireEvent.click(
			screen.getByRole("switch", { name: "force_password_change" }),
		);
		fireEvent.click(screen.getByRole("button", { name: "save_changes" }));

		await waitFor(() => {
			expect(onUpdate).toHaveBeenCalledWith(11, {
				must_change_password: false,
			});
		});
	});

	it("does not show a save action when the toggle returns to its original value", () => {
		renderDialog();

		const toggle = screen.getByRole("switch", {
			name: "force_password_change",
		});
		fireEvent.click(toggle);
		expect(
			screen.getByRole("button", { name: "save_changes" }),
		).toBeInTheDocument();
		fireEvent.click(toggle);

		expect(
			screen.queryByRole("button", { name: "save_changes" }),
		).not.toBeInTheDocument();
	});
});
