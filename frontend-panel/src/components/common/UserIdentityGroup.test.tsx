import { render, screen } from "@testing-library/react";
import { describe, expect, it, vi } from "vitest";
import type { UserSummary } from "@/types/api";
import { UserIdentityGroup } from "./UserIdentityGroup";

vi.mock("@/components/common/UserIdentity", () => ({
	UserIdentity: ({
		user,
	}: {
		size?: "sm" | "md";
		user?: UserSummary | null;
	}) => <div>{user?.profile.display_name ?? user?.username ?? "-"}</div>,
}));

function createUser(id: number, displayName: string): UserSummary {
	return {
		id,
		profile: {
			avatar: {
				source: "none",
				url_1024: null,
				url_512: null,
				version: 0,
			},
			display_name: displayName,
		},
		username: displayName.toLowerCase().replaceAll(" ", "-"),
	};
}

describe("UserIdentityGroup", () => {
	it("shows the fallback label when no users or total are provided", () => {
		render(<UserIdentityGroup fallbackLabel="No uploaders" users={[]} />);

		expect(screen.getByText("No uploaders")).toBeInTheDocument();
	});

	it("shows a total-only count when users are absent but total is positive", () => {
		render(<UserIdentityGroup total={4} users={null} />);

		expect(screen.getByText("+4")).toBeInTheDocument();
	});

	it("limits visible users and derives the remaining count from total", () => {
		render(
			<UserIdentityGroup
				className="uploaders"
				limit={2}
				total={5}
				users={[
					createUser(1, "Root User"),
					createUser(2, "Second User"),
					createUser(3, "Hidden User"),
				]}
			/>,
		);

		expect(screen.getByText("Root User")).toBeInTheDocument();
		expect(screen.getByText("Second User")).toBeInTheDocument();
		expect(screen.queryByText("Hidden User")).not.toBeInTheDocument();
		expect(screen.getByText("+3")).toBeInTheDocument();
	});
});
