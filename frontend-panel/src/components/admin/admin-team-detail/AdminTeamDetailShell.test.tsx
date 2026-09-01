import { fireEvent, render, screen } from "@testing-library/react";
import { createRef } from "react";
import { describe, expect, it, vi } from "vitest";
import { AdminTeamDetailShell } from "@/components/admin/admin-team-detail/AdminTeamDetailShell";
import type { UserSummary } from "@/types/api";

vi.mock("react-i18next", () => ({
	useTranslation: () => ({
		t: (key: string) => key,
	}),
}));

function createUserSummary(): UserSummary {
	return {
		id: 1,
		username: "root",
		profile: {
			display_name: "Root",
			avatar: {
				source: "none",
				url_1024: null,
				url_512: null,
				version: 0,
			},
		},
	};
}

function renderShell(
	overrides: Partial<React.ComponentProps<typeof AdminTeamDetailShell>> = {},
) {
	const handlers = {
		onContentScroll: vi.fn(),
		onPageBack: vi.fn(),
		onSidebarScroll: vi.fn(),
		onTabChange: vi.fn(),
	};

	render(
		<AdminTeamDetailShell
			auditSection={<div>audit content</div>}
			contentRef={createRef<HTMLDivElement>()}
			currentPolicyGroupName="Primary"
			currentTab="audit"
			dangerSection={<div>danger content</div>}
			membersSection={<div>members content</div>}
			overviewSection={<div>overview content</div>}
			ownerCount={2}
			managerCount={3}
			panelAnimationClass="animate"
			quota={1024}
			selectedPolicyGroupName="Pinned"
			sidebarRef={createRef<HTMLElement>()}
			team={{
				archived_at: null,
				created_at: "2026-04-01T00:00:00Z",
				created_by: createUserSummary(),
				description: "Team description",
				id: 14,
				member_count: 8,
				name: "Product",
				policy_group_id: 5,
				storage_quota: 1024,
				storage_used: 512,
				updated_at: "2026-04-02T00:00:00Z",
			}}
			usagePercentage={50}
			used={512}
			{...handlers}
			{...overrides}
		/>,
	);

	return handlers;
}

describe("AdminTeamDetailShell", () => {
	it("keeps the audit section in page layout and forwards tab/back actions", () => {
		const handlers = renderShell();

		expect(screen.getByText("audit content")).toBeInTheDocument();
		expect(screen.queryByText("overview content")).not.toBeInTheDocument();
		expect(screen.getByText("Pinned")).toBeInTheDocument();
		expect(screen.queryByText("Primary")).not.toBeInTheDocument();

		fireEvent.click(screen.getByRole("button", { name: "core:back" }));
		fireEvent.click(
			screen.getByRole("tab", { name: "settings:settings_team_members" }),
		);

		expect(handlers.onPageBack).toHaveBeenCalledTimes(1);
		expect(handlers.onTabChange).toHaveBeenCalledWith(
			"members",
			expect.anything(),
		);
	});
});
