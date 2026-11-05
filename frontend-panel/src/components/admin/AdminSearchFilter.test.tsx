import { fireEvent, render, screen, waitFor } from "@testing-library/react";
import { beforeEach, describe, expect, it, vi } from "vitest";
import { AdminSearchFilter } from "@/components/admin/AdminSearchFilter";

const lookup = vi.hoisted(() => ({
	options: [{ id: 7, label: "Archive", description: "#7" }],
	selected: null,
	selectedLoading: false,
	search: "",
	setSearch: vi.fn(),
	rememberSelection: vi.fn(),
	loading: false,
	failed: false,
	hasMore: false,
	loadMore: vi.fn(),
	retry: vi.fn(),
}));
vi.mock("@/hooks/useAdminLookup", () => ({ useAdminLookup: () => lookup }));
vi.mock("react-i18next", () => ({
	useTranslation: () => ({ t: (key: string) => key }),
}));

beforeEach(() => vi.clearAllMocks());
describe("AdminSearchFilter", () => {
	it.each([
		["policies", "filter_search_policy_placeholder"],
		["users", "filter_search_placeholder"],
	] as const)(
		"searches %s with the right accessible prompt and closes on selection",
		async (kind, prompt) => {
			const onChange = vi.fn();
			render(
				<AdminSearchFilter
					kind={kind}
					value={undefined}
					label="Filter"
					onChange={onChange}
				/>,
			);
			fireEvent.click(screen.getByRole("combobox", { name: "Filter" }));
			const input = await screen.findByPlaceholderText(prompt);
			expect(input).toHaveAccessibleName(prompt);
			fireEvent.change(input, { target: { value: "Arch" } });
			expect(lookup.setSearch).toHaveBeenCalledWith("Arch");
			fireEvent.click(screen.getByRole("option", { name: /Archive/ }));
			expect(onChange).toHaveBeenCalledExactlyOnceWith(7);
			expect(lookup.rememberSelection).toHaveBeenCalledWith(lookup.options[0]);
			await waitFor(() =>
				expect(screen.queryByRole("listbox")).not.toBeInTheDocument(),
			);
		},
	);
});
