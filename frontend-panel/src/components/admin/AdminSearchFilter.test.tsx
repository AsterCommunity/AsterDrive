import { fireEvent, render, screen, waitFor } from "@testing-library/react";
import { expect, it, vi } from "vitest";
import { AdminSearchFilter } from "@/components/admin/AdminSearchFilter";

const mockState = vi.hoisted(() => ({
	getPolicy: vi.fn(),
	listPolicies: vi.fn(),
}));

vi.mock("react-i18next", () => ({
	useTranslation: () => ({ t: (key: string) => key }),
}));

vi.mock("@/services/adminService", () => ({
	adminPolicyService: {
		get: mockState.getPolicy,
		list: mockState.listPolicies,
	},
	adminUserService: { get: vi.fn(), list: vi.fn() },
}));

it("keeps a selected policy until its URL value catches up", async () => {
	mockState.listPolicies.mockResolvedValue({
		items: [{ id: 3, name: "Archive" }],
	});
	const onChange = vi.fn();
	const { rerender } = render(
		<AdminSearchFilter kind="policy" onChange={onChange} />,
	);
	fireEvent.click(screen.getByRole("button", { name: "admin_filter_policy" }));
	fireEvent.click(await screen.findByRole("button", { name: "Archive" }));
	expect(onChange).toHaveBeenCalledWith(3);

	rerender(<AdminSearchFilter kind="policy" value={3} onChange={onChange} />);
	await waitFor(() =>
		expect(screen.getByRole("button", { name: "Archive" })).toBeInTheDocument(),
	);
	expect(mockState.getPolicy).not.toHaveBeenCalled();
});
