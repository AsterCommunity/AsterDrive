import { render, screen, waitFor } from "@testing-library/react";
import { expect, it, vi } from "vitest";
import { AdminUserFilter } from "@/components/admin/AdminUserFilter";

const mockState = vi.hoisted(() => ({ get: vi.fn() }));

vi.mock("react-i18next", () => ({
	useTranslation: () => ({ t: (key: string) => key }),
}));

vi.mock("@/services/adminService", () => ({
	adminUserService: { get: mockState.get, list: vi.fn() },
}));

it("shows the active user ID when its details cannot be loaded", async () => {
	mockState.get.mockRejectedValueOnce(new Error("Unavailable"));
	render(<AdminUserFilter value={9} onChange={vi.fn()} />);

	await waitFor(() => expect(mockState.get).toHaveBeenCalledWith(9));
	expect(screen.getByRole("button", { name: "#9" })).toBeInTheDocument();
});
