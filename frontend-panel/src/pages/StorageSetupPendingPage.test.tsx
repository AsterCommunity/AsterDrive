import { fireEvent, render, screen } from "@testing-library/react";
import { describe, expect, it, vi } from "vitest";
import StorageSetupPendingPage from "./StorageSetupPendingPage";

const logout = vi.hoisted(() => vi.fn(async () => undefined));

vi.mock("react-i18next", () => ({
	useTranslation: () => ({ t: (key: string) => key }),
}));

vi.mock("@/stores/authStore", () => ({
	useAuthStore: (selector: (state: { logout: typeof logout }) => unknown) =>
		selector({ logout }),
}));

describe("StorageSetupPendingPage", () => {
	it("explains the blocked state and keeps logout available", () => {
		render(<StorageSetupPendingPage />);

		expect(screen.getByText("storage_setup_pending_title")).toBeInTheDocument();
		expect(
			screen.getByText("storage_setup_pending_refreshing"),
		).toBeInTheDocument();
		fireEvent.click(screen.getByRole("button", { name: "core:logout" }));
		expect(logout).toHaveBeenCalledTimes(1);
	});
});
