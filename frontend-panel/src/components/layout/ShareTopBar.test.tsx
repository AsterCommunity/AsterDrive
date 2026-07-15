import { fireEvent, render, screen } from "@testing-library/react";
import { beforeEach, describe, expect, it, vi } from "vitest";
import { ShareTopBar } from "@/components/layout/ShareTopBar";

const mockState = vi.hoisted(() => ({
	auth: {
		isAuthenticated: false,
		isChecking: false,
		user: null as { username: string } | null,
	},
	music: {
		isPlaying: false,
		queue: [] as Array<{ id: string }>,
		togglePanel: vi.fn(),
	},
}));

vi.mock("react-router-dom", () => ({
	Link: ({
		children,
		to,
		...props
	}: React.ComponentProps<"a"> & { to: string }) => (
		<a href={to} {...props}>
			{children}
		</a>
	),
}));

vi.mock("react-i18next", () => ({
	useTranslation: () => ({
		t: (key: string) => `translated:${key}`,
	}),
}));

vi.mock("@/components/layout/TopBarShell", () => ({
	TopBarShell: ({
		left,
		right,
		onSidebarToggle,
		sidebarOpen,
	}: {
		left: React.ReactNode;
		right: React.ReactNode;
		onSidebarToggle?: () => void;
		sidebarOpen?: boolean;
	}) => (
		<div
			data-testid="share-topbar-shell"
			data-sidebar-open={String(sidebarOpen)}
		>
			{onSidebarToggle ? (
				<button type="button" onClick={onSidebarToggle}>
					toggle-sidebar
				</button>
			) : null}
			<div>{left}</div>
			<div>{right}</div>
		</div>
	),
}));

vi.mock("@/components/layout/HeaderControls", () => ({
	HeaderControls: ({
		homeLabel,
		showHomeButton,
	}: {
		homeLabel?: string;
		showHomeButton?: boolean;
	}) => (
		<div
			data-testid="header-controls"
			data-home-label={homeLabel}
			data-show-home={String(showHomeButton)}
		/>
	),
}));

vi.mock("@/stores/authStore", () => ({
	useAuthStore: (selector: (state: typeof mockState.auth) => unknown) =>
		selector(mockState.auth),
}));

vi.mock("@/stores/musicPlayerStore", () => ({
	useMusicPlayerStore: (selector: (state: typeof mockState.music) => unknown) =>
		selector(mockState.music),
}));

describe("ShareTopBar", () => {
	beforeEach(() => {
		mockState.auth.isAuthenticated = false;
		mockState.auth.isChecking = false;
		mockState.auth.user = null;
		mockState.music.isPlaying = false;
		mockState.music.queue = [];
		mockState.music.togglePanel.mockReset();
	});

	it("renders the system wordmark and a sign-in entry for guests", () => {
		render(<ShareTopBar />);

		expect(screen.getByAltText("translated:app_name")).toBeInTheDocument();
		expect(
			screen.getByRole("link", { name: "translated:auth:go_to_login" }),
		).toHaveAttribute("href", "/login");
		expect(screen.getByText("translated:files:share")).toHaveClass("sr-only");
	});

	it("uses the authenticated account controls after session probing", () => {
		mockState.auth.isAuthenticated = true;
		mockState.auth.user = { username: "alice" };

		render(<ShareTopBar />);

		expect(screen.getByTestId("header-controls")).toHaveAttribute(
			"data-show-home",
			"true",
		);
		expect(screen.getByTestId("header-controls")).toHaveAttribute(
			"data-home-label",
			"translated:auth:go_home",
		);
		expect(screen.queryByRole("link")).not.toBeInTheDocument();
	});

	it("forwards the mobile sidebar state and toggle", () => {
		const onSidebarToggle = vi.fn();
		render(<ShareTopBar mobileOpen onSidebarToggle={onSidebarToggle} />);

		expect(screen.getByTestId("share-topbar-shell")).toHaveAttribute(
			"data-sidebar-open",
			"true",
		);
		fireEvent.click(screen.getByRole("button", { name: "toggle-sidebar" }));
		expect(onSidebarToggle).toHaveBeenCalledTimes(1);
	});

	it("toggles the music player when music is queued", () => {
		mockState.music.queue = [{ id: "track-1" }];

		render(<ShareTopBar />);

		fireEvent.click(
			screen.getByRole("button", {
				name: "translated:files:music_player_open",
			}),
		);

		expect(mockState.music.togglePanel).toHaveBeenCalledTimes(1);
	});
});
