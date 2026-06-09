import { render, screen } from "@testing-library/react";
import { beforeEach, describe, expect, it, vi } from "vitest";
import { AdminPageHeader } from "@/components/layout/AdminPageHeader";

describe("AdminPageHeader", () => {
	beforeEach(() => {
		vi.mocked(window.matchMedia).mockImplementation((query: string) => ({
			matches: false,
			media: query,
			onchange: null,
			addEventListener: vi.fn(),
			removeEventListener: vi.fn(),
			addListener: vi.fn(),
			removeListener: vi.fn(),
			dispatchEvent: vi.fn(),
		}));
	});

	it("renders title only when optional props are omitted", () => {
		render(<AdminPageHeader title="Users" />);

		expect(screen.getByText("Users")).toBeInTheDocument();
		expect(screen.queryByRole("button")).not.toBeInTheDocument();
	});

	it("renders description, actions, and toolbar when provided", () => {
		render(
			<AdminPageHeader
				title="Users"
				description="Manage user accounts"
				actions={<button type="button">Invite</button>}
				toolbar={<button type="button">Filters</button>}
			/>,
		);

		expect(screen.getByText("Manage user accounts")).toBeInTheDocument();
		expect(screen.getAllByRole("button", { name: "Invite" })).toHaveLength(1);
		expect(screen.getAllByRole("button", { name: "Filters" })).toHaveLength(1);
	});

	it("renders a single combined controls row on mobile", () => {
		vi.mocked(window.matchMedia).mockImplementation((query: string) => ({
			matches: query === "(max-width: 767px)",
			media: query,
			onchange: null,
			addEventListener: vi.fn(),
			removeEventListener: vi.fn(),
			addListener: vi.fn(),
			removeListener: vi.fn(),
			dispatchEvent: vi.fn(),
		}));

		const { container } = render(
			<AdminPageHeader
				title="Users"
				actions={<button type="button">Invite</button>}
				toolbar={<button type="button">Filters</button>}
			/>,
		);

		expect(screen.getAllByRole("button", { name: "Invite" })).toHaveLength(1);
		expect(screen.getAllByRole("button", { name: "Filters" })).toHaveLength(1);
		expect(container).toHaveTextContent("Invite");
		expect(container).toHaveTextContent("Filters");
	});
});
