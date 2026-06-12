import { render, screen } from "@testing-library/react";
import { beforeEach, describe, expect, it, vi } from "vitest";
import { AdminRoute } from "./AdminRoute";
import { LoginGuard } from "./LoginGuard";
import { ProtectedRoute } from "./ProtectedRoute";

const mockState = vi.hoisted(() => ({
	ensureI18nNamespaces: vi.fn(),
	isAuthenticated: false,
	isChecking: false,
	loggerWarn: vi.fn(),
	user: null as {
		must_change_password?: boolean;
		role?: string;
	} | null,
}));

vi.mock("react-router-dom", () => ({
	Navigate: ({ replace, to }: { replace?: boolean; to: string }) => (
		<div data-testid="navigate" data-replace={String(Boolean(replace))}>
			{to}
		</div>
	),
	Outlet: () => <div data-testid="outlet">outlet</div>,
}));

vi.mock("@/components/layout/AdminSiteUrlMismatchPrompt", () => ({
	AdminSiteUrlMismatchPrompt: () => (
		<div data-testid="site-url-mismatch-prompt" />
	),
}));

vi.mock("@/i18n", () => ({
	ensureI18nNamespaces: (...args: unknown[]) =>
		mockState.ensureI18nNamespaces(...args),
}));

vi.mock("@/lib/logger", () => ({
	logger: {
		warn: (...args: unknown[]) => mockState.loggerWarn(...args),
	},
}));

vi.mock("@/stores/authStore", () => ({
	useAuthStore: (
		selector: (state: {
			isAuthenticated: boolean;
			isChecking: boolean;
			user: typeof mockState.user;
		}) => unknown,
	) =>
		selector({
			isAuthenticated: mockState.isAuthenticated,
			isChecking: mockState.isChecking,
			user: mockState.user,
		}),
}));

describe("route guards", () => {
	beforeEach(() => {
		mockState.ensureI18nNamespaces.mockReset();
		mockState.ensureI18nNamespaces.mockResolvedValue(undefined);
		mockState.isAuthenticated = false;
		mockState.isChecking = false;
		mockState.loggerWarn.mockReset();
		mockState.user = null;
	});

	it("shows loading while protected routes are checking auth", () => {
		mockState.isChecking = true;

		render(<ProtectedRoute />);

		expect(screen.queryByTestId("navigate")).not.toBeInTheDocument();
		expect(screen.queryByTestId("outlet")).not.toBeInTheDocument();
	});

	it("sends unauthenticated users to login from protected and admin routes", () => {
		const { rerender } = render(<ProtectedRoute />);

		expect(screen.getByTestId("navigate")).toHaveTextContent("/login");
		expect(screen.getByTestId("navigate")).toHaveAttribute(
			"data-replace",
			"true",
		);

		rerender(<AdminRoute />);

		expect(screen.getByTestId("navigate")).toHaveTextContent("/login");
		expect(screen.getByTestId("navigate")).toHaveAttribute(
			"data-replace",
			"true",
		);
	});

	it("sends authenticated users who must change password to the forced-change page", () => {
		mockState.isAuthenticated = true;
		mockState.user = { must_change_password: true, role: "admin" };
		const { rerender } = render(<LoginGuard />);

		expect(screen.getByTestId("navigate")).toHaveTextContent(
			"/force-password-change",
		);
		expect(screen.getByTestId("navigate")).toHaveAttribute(
			"data-replace",
			"true",
		);

		rerender(<ProtectedRoute />);

		expect(screen.getByTestId("navigate")).toHaveTextContent(
			"/force-password-change",
		);

		rerender(<AdminRoute />);

		expect(screen.getByTestId("navigate")).toHaveTextContent(
			"/force-password-change",
		);
	});

	it("routes authenticated non-admin users away from admin pages", () => {
		mockState.isAuthenticated = true;
		mockState.user = { must_change_password: false, role: "user" };

		render(<AdminRoute />);

		expect(screen.getByTestId("navigate")).toHaveTextContent("/");
		expect(screen.getByTestId("navigate")).toHaveAttribute(
			"data-replace",
			"true",
		);
	});

	it("renders guarded content for normal authenticated users and admins", async () => {
		mockState.isAuthenticated = true;
		mockState.isChecking = true;
		mockState.user = { must_change_password: false, role: "admin" };
		const { rerender } = render(<ProtectedRoute />);

		expect(screen.getByTestId("outlet")).toBeInTheDocument();
		expect(screen.getByTestId("outlet").parentElement).toHaveAttribute(
			"aria-busy",
			"true",
		);

		rerender(<AdminRoute />);

		expect(
			await screen.findByTestId("site-url-mismatch-prompt"),
		).toBeInTheDocument();
		expect(mockState.ensureI18nNamespaces).toHaveBeenCalledWith([
			"admin",
			"core",
		]);
		expect(screen.getByTestId("outlet")).toBeInTheDocument();
		expect(screen.getByTestId("outlet").parentElement).toHaveAttribute(
			"aria-busy",
			"true",
		);
	});

	it("continues rendering admin routes if admin locale loading fails", async () => {
		mockState.ensureI18nNamespaces.mockRejectedValueOnce(
			new Error("locale failed"),
		);
		mockState.isAuthenticated = true;
		mockState.user = { must_change_password: false, role: "admin" };

		render(<AdminRoute />);

		expect(
			await screen.findByTestId("site-url-mismatch-prompt"),
		).toBeInTheDocument();
		expect(mockState.loggerWarn).toHaveBeenCalledWith(
			"failed to load admin locale namespaces",
			expect.any(Error),
		);
	});
});
