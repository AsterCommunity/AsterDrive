import { fireEvent, render, screen, waitFor } from "@testing-library/react";
import type { ReactNode } from "react";
import { beforeEach, describe, expect, it, vi } from "vitest";
import AdminUserDetailPage from "@/pages/admin/AdminUserDetailPage";
import type { UpdateUserRequest, UserInfo } from "@/types/api";

const mockState = vi.hoisted(() => ({
	getUser: vi.fn(),
	navigate: vi.fn(),
	toastSuccess: vi.fn(),
	updateUser: vi.fn(),
	userId: "11",
}));

vi.mock("react-router-dom", () => ({
	Navigate: ({ replace, to }: { replace?: boolean; to: string }) => (
		<div data-testid="redirect">{`${to}:${String(replace)}`}</div>
	),
	useNavigate: () => mockState.navigate,
	useParams: () => ({ userId: mockState.userId }),
}));

vi.mock("react-i18next", () => ({
	useTranslation: () => ({
		t: (key: string) => key.replace(/^core:/, ""),
	}),
}));

vi.mock("sonner", () => ({
	toast: {
		success: (...args: unknown[]) => mockState.toastSuccess(...args),
	},
}));

vi.mock("@/components/admin/UserDetailEditor", () => ({
	UserDetailEditor: ({
		onBack,
		onUpdate,
		user,
	}: {
		onBack: () => void;
		onUpdate: (id: number, data: UpdateUserRequest) => Promise<UserInfo>;
		user: UserInfo;
	}) => (
		<div>
			<span>{`editor:${user.username}:${user.role}`}</span>
			<button type="button" onClick={onBack}>
				back
			</button>
			<button
				type="button"
				onClick={() => void onUpdate(user.id, { role: "admin" })}
			>
				update
			</button>
		</div>
	),
}));

vi.mock("@/components/layout/AdminLayout", () => ({
	AdminLayout: ({ children }: { children: ReactNode }) => <div>{children}</div>,
}));

vi.mock("@/components/layout/AdminPageShell", () => ({
	AdminPageShell: ({ children }: { children: ReactNode }) => (
		<main>{children}</main>
	),
}));

vi.mock("@/components/ui/button", () => ({
	Button: ({
		children,
		onClick,
	}: {
		children: ReactNode;
		onClick?: () => void;
	}) => (
		<button type="button" onClick={onClick}>
			{children}
		</button>
	),
}));

vi.mock("@/components/ui/icon", () => ({
	Icon: ({ name }: { name: string }) => <span>{name}</span>,
}));

vi.mock("@/hooks/usePageTitle", () => ({
	usePageTitle: vi.fn(),
}));

vi.mock("@/services/adminService", () => ({
	adminUserService: {
		get: (...args: unknown[]) => mockState.getUser(...args),
		update: (...args: unknown[]) => mockState.updateUser(...args),
	},
}));

function user(overrides: Partial<UserInfo> = {}): UserInfo {
	return {
		created_at: "2026-09-08T00:00:00Z",
		email: "alice@example.com",
		email_verified: true,
		id: 11,
		must_change_password: false,
		pending_email: null,
		policy_group_id: 1,
		profile: {
			avatar: {
				source: "none",
				url_512: null,
				url_1024: null,
				version: 0,
			},
			display_name: "Alice",
		},
		role: "user",
		status: "active",
		storage_quota: 1024,
		storage_used: 512,
		updated_at: "2026-09-08T00:00:00Z",
		username: "alice",
		...overrides,
	};
}

describe("AdminUserDetailPage", () => {
	beforeEach(() => {
		mockState.getUser.mockReset();
		mockState.navigate.mockReset();
		mockState.toastSuccess.mockReset();
		mockState.updateUser.mockReset();
		mockState.userId = "11";
		mockState.getUser.mockResolvedValue(user());
		mockState.updateUser.mockResolvedValue(user({ role: "admin" }));
	});

	it("loads the user by route id and keeps updates on the detail page", async () => {
		render(<AdminUserDetailPage />);

		await waitFor(() => {
			expect(mockState.getUser).toHaveBeenCalledWith(11);
			expect(screen.getByText("editor:alice:user")).toBeInTheDocument();
		});

		fireEvent.click(screen.getByRole("button", { name: "update" }));

		await waitFor(() => {
			expect(mockState.updateUser).toHaveBeenCalledWith(11, { role: "admin" });
			expect(screen.getByText("editor:alice:admin")).toBeInTheDocument();
		});
		expect(mockState.toastSuccess).toHaveBeenCalledWith("user_updated");
	});

	it("returns to the user list without a root view transition", async () => {
		render(<AdminUserDetailPage />);
		await screen.findByText("editor:alice:user");

		fireEvent.click(screen.getByRole("button", { name: "back" }));

		expect(mockState.navigate).toHaveBeenCalledWith("/admin/users", {
			viewTransition: false,
		});
	});

	it("renders a not-found state when the detail request fails", async () => {
		mockState.getUser.mockRejectedValueOnce(new Error("missing"));
		render(<AdminUserDetailPage />);

		expect(await screen.findByText("user_not_found")).toBeInTheDocument();
		expect(
			screen.getByRole("button", { name: /back_to_users/i }),
		).toBeInTheDocument();
	});

	it("redirects invalid user ids to the list", () => {
		mockState.userId = "invalid";
		render(<AdminUserDetailPage />);

		expect(screen.getByTestId("redirect")).toHaveTextContent(
			"/admin/users:true",
		);
		expect(mockState.getUser).not.toHaveBeenCalled();
	});
});
