import { fireEvent, render, screen, waitFor } from "@testing-library/react";
import { beforeEach, expect, it, vi } from "vitest";
import { AdminUserFilter } from "@/components/admin/AdminUserFilter";

const mockState = vi.hoisted(() => ({ get: vi.fn(), list: vi.fn() }));

vi.mock("react-i18next", () => ({
	useTranslation: () => ({ t: (key: string) => key }),
}));

vi.mock("@/services/adminService", () => ({
	adminUserService: { get: mockState.get, list: mockState.list },
}));

beforeEach(() => {
	mockState.get.mockReset();
	mockState.list.mockReset();
});

it("shows the active user ID when its details cannot be loaded", async () => {
	mockState.get.mockRejectedValueOnce(new Error("Unavailable"));
	render(<AdminUserFilter value={9} onChange={vi.fn()} />);

	await waitFor(() => expect(mockState.get).toHaveBeenCalledWith(9));
	expect(screen.getByRole("button", { name: "#9" })).toBeInTheDocument();
});

it("searches, selects, and clears a user", async () => {
	const user = { id: 9, username: "admin", email: "admin@example.com" };
	mockState.list.mockResolvedValue({ items: [user] });
	const onChange = vi.fn();
	const { rerender } = render(<AdminUserFilter onChange={onChange} />);

	fireEvent.click(screen.getByRole("button", { name: "filter_user" }));
	fireEvent.change(screen.getByPlaceholderText("search_user"), {
		target: { value: "admin" },
	});
	await waitFor(() =>
		expect(mockState.list).toHaveBeenCalledWith({
			keyword: "admin",
			limit: 20,
		}),
	);
	fireEvent.click(
		await screen.findByRole("button", { name: "admin (admin@example.com)" }),
	);
	expect(onChange).toHaveBeenCalledWith(9);
	rerender(<AdminUserFilter value={9} onChange={onChange} />);
	expect(
		screen.getAllByRole("button", { name: "admin (admin@example.com)" })[0],
	).toBeInTheDocument();
	expect(mockState.get).not.toHaveBeenCalled();

	fireEvent.click(
		screen.getAllByRole("button", { name: "admin (admin@example.com)" })[0],
	);
	fireEvent.click(screen.getByRole("button", { name: "filter_all_users" }));
	expect(onChange).toHaveBeenLastCalledWith(undefined);
	rerender(<AdminUserFilter onChange={onChange} />);
	expect(
		screen.getByRole("button", { name: "filter_user" }),
	).toBeInTheDocument();
});

it("restores the selected user from its URL value", async () => {
	mockState.get.mockResolvedValue({
		id: 9,
		username: "admin",
		email: "admin@example.com",
	});
	render(<AdminUserFilter value={9} onChange={vi.fn()} />);
	await waitFor(() =>
		expect(
			screen.getByRole("button", { name: "admin (admin@example.com)" }),
		).toBeInTheDocument(),
	);
});

it("does not reload the old user while switching an active filter", async () => {
	mockState.get.mockResolvedValue({
		id: 4,
		username: "first",
		email: "first@example.com",
	});
	mockState.list.mockResolvedValue({
		items: [{ id: 5, username: "next", email: "next@example.com" }],
	});
	const onChange = vi.fn();
	const { rerender } = render(
		<AdminUserFilter value={4} onChange={onChange} />,
	);
	fireEvent.click(
		await screen.findByRole("button", { name: "first (first@example.com)" }),
	);
	fireEvent.click(
		await screen.findByRole("button", { name: "next (next@example.com)" }),
	);
	rerender(<AdminUserFilter value={5} onChange={onChange} />);
	expect(onChange).toHaveBeenCalledWith(5);
	expect(mockState.get).toHaveBeenCalledTimes(1);
});
