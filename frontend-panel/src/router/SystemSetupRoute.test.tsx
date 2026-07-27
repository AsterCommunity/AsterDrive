import {
	act,
	fireEvent,
	render,
	screen,
	waitFor,
} from "@testing-library/react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import {
	PendingSystemSetupRoute,
	ReadySystemSetupRoute,
	StorageSystemSetupRoute,
} from "./SystemSetupRoute";

const mockState = vi.hoisted(() => ({
	error: null as unknown,
	isAuthenticated: true,
	isChecking: false,
	mustChangePassword: false,
	refresh: vi.fn(async () => "needs_storage" as const),
	setupChecking: false,
	setupState: "needs_storage" as
		| "needs_admin"
		| "needs_storage"
		| "ready"
		| null,
	userRole: "admin" as "admin" | "user",
}));

vi.mock("react-i18next", () => ({
	useTranslation: () => ({ t: (key: string) => key }),
}));

vi.mock("react-router-dom", () => ({
	Navigate: ({
		replace,
		to,
	}: {
		replace?: boolean;
		to: string | { pathname: string; search?: string };
	}) => (
		<div data-testid="navigate" data-replace={String(Boolean(replace))}>
			{typeof to === "string" ? to : `${to.pathname}${to.search ?? ""}`}
		</div>
	),
	Outlet: () => <div data-testid="outlet">outlet</div>,
	useLocation: () => ({ search: "?storage_authorization=success&policy_id=7" }),
}));

vi.mock("@/stores/authStore", () => ({
	useAuthStore: (
		selector: (state: {
			isAuthenticated: boolean;
			isChecking: boolean;
			user: {
				must_change_password: boolean;
				role: "admin" | "user";
			};
		}) => unknown,
	) =>
		selector({
			isAuthenticated: mockState.isAuthenticated,
			isChecking: mockState.isChecking,
			user: {
				must_change_password: mockState.mustChangePassword,
				role: mockState.userRole,
			},
		}),
}));

vi.mock("@/stores/systemSetupStore", () => ({
	useSystemSetupStore: (
		selector: (state: {
			error: unknown;
			isChecking: boolean;
			refresh: typeof mockState.refresh;
			setupState: typeof mockState.setupState;
		}) => unknown,
	) =>
		selector({
			error: mockState.error,
			isChecking: mockState.setupChecking,
			refresh: mockState.refresh,
			setupState: mockState.setupState,
		}),
}));

describe("system setup route guards", () => {
	beforeEach(() => {
		mockState.error = null;
		mockState.isAuthenticated = true;
		mockState.isChecking = false;
		mockState.mustChangePassword = false;
		mockState.refresh.mockReset();
		mockState.refresh.mockResolvedValue("needs_storage");
		mockState.setupChecking = false;
		mockState.setupState = "needs_storage";
		mockState.userRole = "admin";
	});

	afterEach(() => {
		vi.useRealTimers();
	});

	it("redirects normal routes to storage setup while initialization is open", async () => {
		const { rerender } = render(<ReadySystemSetupRoute />);

		expect(screen.getByTestId("navigate")).toHaveTextContent(
			"/setup/storage?storage_authorization=success&policy_id=7",
		);
		await waitFor(() => expect(mockState.refresh).toHaveBeenCalled());

		mockState.userRole = "user";
		rerender(<ReadySystemSetupRoute />);
		expect(screen.getByTestId("navigate")).toHaveTextContent("/setup/pending");
	});

	it("renders normal routes only after setup becomes ready", () => {
		mockState.setupState = "ready";

		render(<ReadySystemSetupRoute />);

		expect(screen.getByTestId("outlet")).toBeInTheDocument();
	});

	it("protects the administrator storage setup route", () => {
		const { rerender } = render(<StorageSystemSetupRoute />);
		expect(screen.getByTestId("outlet")).toBeInTheDocument();

		mockState.userRole = "user";
		rerender(<StorageSystemSetupRoute />);
		expect(screen.getByTestId("navigate")).toHaveTextContent("/setup/pending");

		mockState.userRole = "admin";
		mockState.setupState = "ready";
		rerender(<StorageSystemSetupRoute />);
		expect(screen.getByTestId("navigate")).toHaveTextContent("/");

		mockState.setupState = "needs_admin";
		rerender(<StorageSystemSetupRoute />);
		expect(screen.getByTestId("navigate")).toHaveTextContent("/login");
	});

	it("preserves authentication and password-change precedence", () => {
		mockState.isAuthenticated = false;
		const { rerender } = render(<StorageSystemSetupRoute />);
		expect(screen.getByTestId("navigate")).toHaveTextContent("/login");

		mockState.isAuthenticated = true;
		mockState.mustChangePassword = true;
		rerender(<StorageSystemSetupRoute />);
		expect(screen.getByTestId("navigate")).toHaveTextContent(
			"/force-password-change",
		);

		rerender(<PendingSystemSetupRoute />);
		expect(screen.getByTestId("navigate")).toHaveTextContent(
			"/force-password-change",
		);
	});

	it("keeps non-admin users on the pending route until setup is ready", () => {
		mockState.userRole = "user";
		const { rerender } = render(<PendingSystemSetupRoute />);
		expect(screen.getByTestId("outlet")).toBeInTheDocument();

		mockState.userRole = "admin";
		rerender(<PendingSystemSetupRoute />);
		expect(screen.getByTestId("navigate")).toHaveTextContent("/setup/storage");

		mockState.setupState = "ready";
		rerender(<PendingSystemSetupRoute />);
		expect(screen.getByTestId("navigate")).toHaveTextContent("/");
	});

	it("polls while storage setup is incomplete and stops after it becomes ready", async () => {
		vi.useFakeTimers();
		const { rerender } = render(<StorageSystemSetupRoute />);
		await act(async () => undefined);
		const initialCalls = mockState.refresh.mock.calls.length;

		await act(async () => {
			vi.advanceTimersByTime(2_000);
		});
		expect(mockState.refresh.mock.calls.length).toBeGreaterThan(initialCalls);

		mockState.setupState = "ready";
		rerender(<StorageSystemSetupRoute />);
		const callsAfterReady = mockState.refresh.mock.calls.length;
		await act(async () => {
			vi.advanceTimersByTime(4_000);
		});
		expect(mockState.refresh).toHaveBeenCalledTimes(callsAfterReady);
	});

	it("refreshes on focus and visible-page transitions, then removes listeners", async () => {
		const visibilityDescriptor = Object.getOwnPropertyDescriptor(
			document,
			"visibilityState",
		);
		const setVisibility = (value: "hidden" | "visible") => {
			Object.defineProperty(document, "visibilityState", {
				configurable: true,
				value,
			});
		};
		setVisibility("visible");
		const { unmount } = render(<StorageSystemSetupRoute />);
		await waitFor(() => expect(mockState.refresh).toHaveBeenCalled());
		mockState.refresh.mockClear();

		fireEvent.focus(window);
		expect(mockState.refresh).toHaveBeenCalledTimes(1);

		setVisibility("hidden");
		fireEvent(document, new Event("visibilitychange"));
		expect(mockState.refresh).toHaveBeenCalledTimes(1);

		setVisibility("visible");
		fireEvent(document, new Event("visibilitychange"));
		expect(mockState.refresh).toHaveBeenCalledTimes(2);

		unmount();
		mockState.refresh.mockClear();
		fireEvent.focus(window);
		fireEvent(document, new Event("visibilitychange"));
		expect(mockState.refresh).not.toHaveBeenCalled();

		if (visibilityDescriptor) {
			Object.defineProperty(document, "visibilityState", visibilityDescriptor);
		}
	});

	it("does not query or poll setup state before authentication is usable", async () => {
		vi.useFakeTimers();
		mockState.isAuthenticated = false;
		const { rerender } = render(<StorageSystemSetupRoute />);

		await act(async () => {
			vi.advanceTimersByTime(4_000);
		});
		fireEvent.focus(window);
		expect(mockState.refresh).not.toHaveBeenCalled();

		mockState.isAuthenticated = true;
		mockState.mustChangePassword = true;
		rerender(<StorageSystemSetupRoute />);
		await act(async () => {
			vi.advanceTimersByTime(4_000);
		});
		fireEvent.focus(window);
		expect(mockState.refresh).not.toHaveBeenCalled();
	});

	it("keeps an already known setup state usable during a transient refresh failure", () => {
		mockState.error = new Error("temporary failure");
		mockState.setupState = "needs_storage";

		render(<StorageSystemSetupRoute />);

		expect(screen.getByTestId("outlet")).toBeInTheDocument();
		expect(
			screen.queryByText("storage_setup_state_load_failed_title"),
		).not.toBeInTheDocument();
	});

	it("shows a retry surface when the first setup-state request fails", () => {
		mockState.setupState = null;
		mockState.error = new Error("network down");

		render(<ReadySystemSetupRoute />);

		expect(
			screen.getByText("storage_setup_state_load_failed_title"),
		).toBeInTheDocument();
		fireEvent.click(screen.getByRole("button"));
		expect(mockState.refresh).toHaveBeenCalled();
	});
});
