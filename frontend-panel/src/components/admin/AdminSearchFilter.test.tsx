import { fireEvent, render, screen, waitFor } from "@testing-library/react";
import { beforeEach, expect, it, vi } from "vitest";
import { AdminSearchFilter } from "@/components/admin/AdminSearchFilter";

const mockState = vi.hoisted(() => ({
	getPolicy: vi.fn(),
	getUser: vi.fn(),
	listPolicies: vi.fn(),
	listUsers: vi.fn(),
}));

vi.mock("react-i18next", () => ({
	useTranslation: () => ({ t: (key: string) => key }),
}));

vi.mock("@/services/adminService", () => ({
	adminPolicyService: {
		get: mockState.getPolicy,
		list: mockState.listPolicies,
	},
	adminUserService: { get: mockState.getUser, list: mockState.listUsers },
}));

beforeEach(() => {
	mockState.getPolicy.mockReset();
	mockState.getUser.mockReset();
	mockState.listPolicies.mockReset();
	mockState.listUsers.mockReset();
});

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

it("restores a policy from the URL, searches, and clears it", async () => {
	mockState.getPolicy.mockResolvedValue({ id: 3, name: "Archive" });
	mockState.listPolicies.mockResolvedValue({
		items: [{ id: 3, name: "Archive" }],
	});
	const onChange = vi.fn();
	const { rerender } = render(
		<AdminSearchFilter kind="policy" value={3} onChange={onChange} />,
	);
	await waitFor(() =>
		expect(screen.getByRole("button", { name: "Archive" })).toBeInTheDocument(),
	);
	fireEvent.click(screen.getByRole("button", { name: "Archive" }));
	fireEvent.change(screen.getByPlaceholderText("admin_filter_policy"), {
		target: { value: "Archive" },
	});
	await waitFor(() =>
		expect(mockState.listPolicies).toHaveBeenCalledWith({
			keyword: "Archive",
			limit: 20,
		}),
	);
	fireEvent.click(screen.getByRole("button", { name: "admin_filter_all" }));
	expect(onChange).toHaveBeenCalledWith(undefined);
	rerender(<AdminSearchFilter kind="policy" onChange={onChange} />);
	expect(
		screen.getByRole("button", { name: "admin_filter_policy" }),
	).toBeInTheDocument();
});

it("loads user options and retains the chosen user during URL updates", async () => {
	mockState.getUser.mockResolvedValue({
		id: 4,
		username: "first",
		email: "first@example.com",
	});
	mockState.listUsers.mockResolvedValue({
		items: [{ id: 5, username: "root", email: "root@example.com" }],
	});
	const onChange = vi.fn();
	const { rerender } = render(
		<AdminSearchFilter kind="user" value={4} onChange={onChange} />,
	);
	await waitFor(() =>
		expect(
			screen.getByRole("button", { name: "first (first@example.com)" }),
		).toBeInTheDocument(),
	);
	fireEvent.click(
		screen.getByRole("button", { name: "first (first@example.com)" }),
	);
	fireEvent.change(screen.getByPlaceholderText("admin_filter_user"), {
		target: { value: "root" },
	});
	await waitFor(() =>
		expect(mockState.listUsers).toHaveBeenCalledWith({
			keyword: "root",
			limit: 20,
		}),
	);
	fireEvent.click(
		await screen.findByRole("button", { name: "root (root@example.com)" }),
	);
	expect(onChange).toHaveBeenCalledWith(5);
	rerender(<AdminSearchFilter kind="user" value={5} onChange={onChange} />);
	expect(
		screen.getAllByRole("button", { name: "root (root@example.com)" })[0],
	).toBeInTheDocument();
	expect(mockState.getUser).toHaveBeenCalledTimes(1);
});

it("keeps the active ID visible when a saved selection cannot load", async () => {
	mockState.getPolicy.mockRejectedValue(new Error("Unavailable"));
	mockState.listPolicies.mockRejectedValue(new Error("Unavailable"));
	render(<AdminSearchFilter kind="policy" value={3} onChange={vi.fn()} />);
	await waitFor(() => expect(mockState.getPolicy).toHaveBeenCalledWith(3));
	expect(screen.getByRole("button", { name: "#3" })).toBeInTheDocument();
	fireEvent.click(screen.getByRole("button", { name: "#3" }));
	await waitFor(() => expect(mockState.listPolicies).toHaveBeenCalled());
});
