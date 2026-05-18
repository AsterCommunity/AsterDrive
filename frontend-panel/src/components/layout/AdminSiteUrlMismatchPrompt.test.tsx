import { fireEvent, render, screen, waitFor } from "@testing-library/react";
import { beforeEach, describe, expect, it, vi } from "vitest";
import { AdminSiteUrlMismatchPrompt } from "@/components/layout/AdminSiteUrlMismatchPrompt";
import { setPublicSiteUrls } from "@/lib/publicSiteUrl";

const mockState = vi.hoisted(() => ({
	brandingLoaded: false,
	getConfig: vi.fn(),
	handleApiError: vi.fn(),
	loggerWarn: vi.fn(),
	navigate: vi.fn(),
	setConfig: vi.fn(),
	siteUrl: null as string | null,
	toastSuccess: vi.fn(),
}));

const defaultLocation = window.location;

function AdminRouteShell({ page }: { page: string }) {
	return (
		<div>
			<AdminSiteUrlMismatchPrompt />
			<div>{page}</div>
		</div>
	);
}

vi.mock("react-i18next", () => ({
	useTranslation: () => ({
		t: (key: string, options?: Record<string, unknown>) =>
			options
				? `translated:${key}:${JSON.stringify(options)}`
				: `translated:${key}`,
	}),
}));

vi.mock("sonner", () => ({
	toast: {
		success: (...args: unknown[]) => mockState.toastSuccess(...args),
	},
}));

vi.mock("react-router-dom", () => ({
	useNavigate: () => mockState.navigate,
}));

vi.mock("@/components/common/ConfirmDialog", () => ({
	ConfirmDialog: ({
		confirmLabel,
		description,
		onConfirm,
		onOpenChange,
		open,
		title,
	}: {
		confirmLabel?: string;
		description?: string;
		onConfirm: () => void;
		onOpenChange: (open: boolean) => void;
		open: boolean;
		title: string;
	}) =>
		open ? (
			<div>
				<h2>{title}</h2>
				{description ? <p>{description}</p> : null}
				<button type="button" onClick={() => onOpenChange(false)}>
					cancel
				</button>
				<button type="button" onClick={onConfirm}>
					{confirmLabel ?? "confirm"}
				</button>
			</div>
		) : null,
}));

vi.mock("@/hooks/useApiError", () => ({
	handleApiError: (...args: unknown[]) => mockState.handleApiError(...args),
}));

vi.mock("@/lib/logger", () => ({
	logger: {
		warn: (...args: unknown[]) => mockState.loggerWarn(...args),
		error: vi.fn(),
		debug: vi.fn(),
	},
}));

vi.mock("@/services/adminService", () => ({
	adminConfigService: {
		get: (...args: unknown[]) => mockState.getConfig(...args),
		set: (...args: unknown[]) => mockState.setConfig(...args),
	},
}));

vi.mock("@/stores/brandingStore", () => {
	const useBrandingStore = ((
		selector: (state: { isLoaded: boolean; siteUrl: string | null }) => unknown,
	) =>
		selector({
			isLoaded: mockState.brandingLoaded,
			siteUrl: mockState.siteUrl,
		})) as unknown as typeof import("@/stores/brandingStore").useBrandingStore;

	useBrandingStore.setState = (partial: { siteUrl?: string | null }) => {
		if ("siteUrl" in partial) {
			mockState.siteUrl = partial.siteUrl ?? null;
		}
	};

	return { useBrandingStore };
});

describe("AdminSiteUrlMismatchPrompt", () => {
	beforeEach(() => {
		Object.defineProperty(window, "location", {
			configurable: true,
			value: defaultLocation,
		});
		mockState.brandingLoaded = false;
		mockState.getConfig.mockReset();
		mockState.handleApiError.mockReset();
		mockState.loggerWarn.mockReset();
		mockState.navigate.mockReset();
		mockState.setConfig.mockReset();
		mockState.siteUrl = null;
		mockState.toastSuccess.mockReset();
		mockState.getConfig.mockResolvedValue({
			key: "public_site_url",
			value: ["https://configured.example.com"],
		});
		mockState.setConfig.mockResolvedValue({
			key: "public_site_url",
			value: [window.location.origin],
		});
		setPublicSiteUrls(null);
	});

	it("does not reopen while the admin route shell stays mounted", async () => {
		mockState.brandingLoaded = true;
		mockState.siteUrl = "https://configured.example.com";

		const { rerender } = render(<AdminRouteShell page="Users" />);

		expect(
			await screen.findByText("translated:site_url_mismatch_title"),
		).toBeInTheDocument();

		fireEvent.click(screen.getByRole("button", { name: "cancel" }));
		expect(
			screen.queryByText("translated:site_url_mismatch_title"),
		).not.toBeInTheDocument();

		rerender(<AdminRouteShell page="Settings" />);

		expect(screen.getByText("Settings")).toBeInTheDocument();
		expect(
			screen.queryByText("translated:site_url_mismatch_title"),
		).not.toBeInTheDocument();
	});

	it("shows the prompt again after leaving admin and can update the config", async () => {
		mockState.brandingLoaded = true;
		mockState.siteUrl = "https://configured.example.com";

		const { unmount } = render(<AdminSiteUrlMismatchPrompt />);

		expect(
			await screen.findByText("translated:site_url_mismatch_title"),
		).toBeInTheDocument();

		fireEvent.click(screen.getByRole("button", { name: "cancel" }));
		expect(
			screen.queryByText("translated:site_url_mismatch_title"),
		).not.toBeInTheDocument();

		unmount();
		render(<AdminSiteUrlMismatchPrompt />);

		expect(
			await screen.findByText("translated:site_url_mismatch_title"),
		).toBeInTheDocument();

		fireEvent.click(
			screen.getByRole("button", {
				name: "translated:site_url_mismatch_confirm",
			}),
		);

		await waitFor(() => {
			expect(mockState.setConfig).toHaveBeenCalledWith("public_site_url", [
				"https://configured.example.com",
				window.location.origin,
			]);
		});
		expect(mockState.toastSuccess).toHaveBeenCalledWith(
			"translated:settings_saved",
		);
	});

	it("does not prompt when the current origin exists in the configured origin list", async () => {
		mockState.brandingLoaded = true;
		mockState.siteUrl = "http://localhost:3000";
		mockState.getConfig.mockResolvedValue({
			key: "public_site_url",
			value: ["http://localhost:3000", window.location.origin],
		});

		render(<AdminSiteUrlMismatchPrompt />);

		await waitFor(() => {
			expect(mockState.getConfig).toHaveBeenCalledWith("public_site_url");
		});
		expect(
			screen.queryByText("translated:site_url_mismatch_title"),
		).not.toBeInTheDocument();
		expect(mockState.navigate).not.toHaveBeenCalled();
		expect(mockState.setConfig).not.toHaveBeenCalled();
	});

	it("normalizes legacy single-string public site urls", async () => {
		mockState.brandingLoaded = true;
		mockState.siteUrl = null;
		mockState.getConfig.mockResolvedValue({
			key: "public_site_url",
			value: " https://configured.example.com/ ",
		});
		Object.defineProperty(window, "location", {
			configurable: true,
			value: {
				...defaultLocation,
				origin: "https://preview.example.com",
			},
		});

		render(<AdminSiteUrlMismatchPrompt />);

		expect(
			await screen.findByText("translated:site_url_mismatch_title"),
		).toBeInTheDocument();
		expect(
			screen.getByText(/https:\/\/configured\.example\.com/),
		).toBeInTheDocument();
	});

	it("uses the live admin config instead of stale public branding cache", async () => {
		mockState.brandingLoaded = true;
		mockState.siteUrl = null;
		mockState.getConfig.mockResolvedValue({
			key: "public_site_url",
			value: [window.location.origin],
		});

		render(<AdminSiteUrlMismatchPrompt />);

		await waitFor(() => {
			expect(mockState.getConfig).toHaveBeenCalledWith("public_site_url");
		});
		expect(mockState.siteUrl).toBe(window.location.origin);
		expect(
			screen.queryByText("translated:site_url_mismatch_title"),
		).not.toBeInTheDocument();
		expect(mockState.navigate).not.toHaveBeenCalled();
		expect(mockState.setConfig).not.toHaveBeenCalled();
	});

	it("keeps the prompt when no origins are configured", async () => {
		mockState.brandingLoaded = true;
		mockState.siteUrl = null;
		mockState.getConfig.mockResolvedValue({
			key: "public_site_url",
			value: [],
		});

		render(<AdminSiteUrlMismatchPrompt />);

		expect(
			await screen.findByText("translated:site_url_mismatch_title"),
		).toBeInTheDocument();
		expect(mockState.navigate).not.toHaveBeenCalled();
	});

	it("redirects to settings instead of prompting when multiple origins are configured and the current origin is unknown", async () => {
		Object.defineProperty(window, "location", {
			configurable: true,
			value: {
				...defaultLocation,
				origin: "http://localhost:5174",
			},
		});
		mockState.brandingLoaded = true;
		mockState.siteUrl = "http://localhost:3000";
		mockState.getConfig.mockResolvedValue({
			key: "public_site_url",
			value: ["http://localhost:3000", "http://localhost:5173"],
		});

		render(<AdminSiteUrlMismatchPrompt />);

		await waitFor(() => {
			expect(mockState.navigate).toHaveBeenCalledWith(
				"/admin/settings/general",
				{ replace: true },
			);
		});
		expect(
			screen.queryByText("translated:site_url_mismatch_title"),
		).not.toBeInTheDocument();
		expect(mockState.setConfig).not.toHaveBeenCalled();
	});
});
