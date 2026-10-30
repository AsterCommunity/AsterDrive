import { fireEvent, render, screen, waitFor } from "@testing-library/react";
import { beforeEach, describe, expect, it, vi } from "vitest";
import { AdminWorkspaceSidebar } from "./AdminWorkspaceSidebar";

const mocks = vi.hoisted(() => ({ list: vi.fn(), error: vi.fn() }));
vi.mock("@/services/adminContentAccessService", () => ({
	adminContentAccessService: mocks,
}));
vi.mock("@/hooks/useApiError", () => ({ handleApiError: mocks.error }));
vi.mock("react-i18next", () => ({
	useTranslation: () => ({ t: (key: string) => key }),
}));
vi.mock("@/components/common/UserAvatarImage", () => ({
	UserAvatarImage: () => <span>target avatar</span>,
}));
describe("scoped workspace sidebar", () => {
	beforeEach(() => {
		vi.resetAllMocks();
		mocks.list.mockResolvedValue({
			contents: { folders: [], folders_total: 0 },
		});
	});
	it("never requests directory data before access exists", () => {
		render(<AdminWorkspaceSidebar />);
		expect(mocks.list).not.toHaveBeenCalled();
	});
	it("uses the same access for root, nested tree and navigation with file reads disabled", async () => {
		mocks.list.mockResolvedValueOnce({
			contents: {
				folders: [{ id: 9, name: "target-folder", icon: { kind: "default" } }],
				folders_total: 1,
			},
		});
		const navigate = vi.fn();
		render(<AdminWorkspaceSidebar accessId="grant" onNavigate={navigate} />);
		fireEvent.click(
			await screen.findByRole("button", { name: "target-folder" }),
		);
		expect(navigate).toHaveBeenCalledWith(9);
		expect(mocks.list).toHaveBeenCalledWith(
			expect.objectContaining({
				access_id: "grant",
				file_limit: 0,
				folder_limit: 100,
			}),
		);
		fireEvent.click(screen.getByRole("button", { name: "expand_tree" }));
		await waitFor(() =>
			expect(mocks.list).toHaveBeenCalledWith(
				expect.objectContaining({
					access_id: "grant",
					folder_id: 9,
					file_limit: 0,
				}),
			),
		);
	});
});
