import { fireEvent, render, screen, waitFor } from "@testing-library/react";
import { beforeEach, describe, expect, it, vi } from "vitest";
import StorageSetupPage from "./StorageSetupPage";

const mockState = vi.hoisted(() => ({
	handleApiError: vi.fn(),
	logout: vi.fn(),
	policyPageRenderCount: 0,
}));

vi.mock("react-i18next", () => ({
	useTranslation: () => ({ t: (key: string) => key }),
}));

vi.mock("@/components/common/AsterDriveWordmark", () => ({
	AsterDriveWordmark: ({ alt }: { alt: string }) => <img alt={alt} />,
}));

vi.mock("@/hooks/useApiError", () => ({
	handleApiError: (...args: unknown[]) => mockState.handleApiError(...args),
}));

vi.mock("@/hooks/usePageTitle", () => ({
	usePageTitle: vi.fn(),
}));

vi.mock("@/stores/authStore", () => ({
	useAuthStore: (
		selector: (state: { logout: typeof mockState.logout }) => unknown,
	) => selector({ logout: mockState.logout }),
}));

vi.mock("@/pages/admin/AdminPoliciesPage", () => ({
	default: ({ variant }: { variant: string }) => {
		mockState.policyPageRenderCount += 1;
		return <div data-testid="admin-policies-page">{variant}</div>;
	},
}));

describe("StorageSetupPage", () => {
	beforeEach(() => {
		mockState.handleApiError.mockReset();
		mockState.logout.mockReset();
		mockState.logout.mockResolvedValue(undefined);
		mockState.policyPageRenderCount = 0;
	});

	it("explains the initialization before showing storage policy controls", () => {
		render(<StorageSetupPage />);

		expect(screen.getByRole("img", { name: "AsterDrive" })).toBeInTheDocument();
		expect(screen.getByText("storage_setup_eyebrow")).toBeInTheDocument();
		expect(
			screen.getByRole("heading", { name: "storage_setup_page_title" }),
		).toBeInTheDocument();
		expect(screen.getByText("storage_setup_page_desc")).toBeInTheDocument();
		expect(
			screen.getByRole("button", { name: "storage_setup_start" }),
		).toBeInTheDocument();
		expect(screen.queryByTestId("admin-policies-page")).not.toBeInTheDocument();
		expect(mockState.policyPageRenderCount).toBe(0);
	});

	it("opens the existing setup policy flow only after an explicit action", () => {
		render(<StorageSetupPage />);

		fireEvent.click(
			screen.getByRole("button", { name: "storage_setup_start" }),
		);

		expect(screen.getByTestId("admin-policies-page")).toHaveTextContent(
			"setup",
		);
		expect(
			screen.queryByRole("button", { name: "storage_setup_start" }),
		).not.toBeInTheDocument();
		expect(mockState.policyPageRenderCount).toBe(1);
	});

	it("logs out from the initialization explanation page", async () => {
		render(<StorageSetupPage />);

		fireEvent.click(screen.getByRole("button", { name: "core:logout" }));

		await waitFor(() => {
			expect(mockState.logout).toHaveBeenCalledTimes(1);
		});
	});

	it("prevents duplicate logout requests while one is pending", async () => {
		let resolveLogout!: () => void;
		mockState.logout.mockReturnValueOnce(
			new Promise<void>((resolve) => {
				resolveLogout = resolve;
			}),
		);
		render(<StorageSetupPage />);

		const logoutButton = screen.getByRole("button", { name: "core:logout" });
		fireEvent.click(logoutButton);
		fireEvent.click(logoutButton);

		expect(mockState.logout).toHaveBeenCalledTimes(1);
		expect(logoutButton).toBeDisabled();
		resolveLogout();
	});

	it("reports logout failures and restores the logout action", async () => {
		const error = new Error("logout failed");
		mockState.logout.mockRejectedValueOnce(error);
		render(<StorageSetupPage />);

		const logoutButton = screen.getByRole("button", { name: "core:logout" });
		fireEvent.click(logoutButton);

		await waitFor(() => {
			expect(mockState.handleApiError).toHaveBeenCalledWith(error);
		});
		expect(logoutButton).toBeEnabled();
	});
});
