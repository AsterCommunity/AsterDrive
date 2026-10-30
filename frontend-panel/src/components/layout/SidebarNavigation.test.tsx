import { fireEvent, render, screen } from "@testing-library/react";
import { MemoryRouter } from "react-router-dom";
import { expect, it, vi } from "vitest";
import { SidebarNavigation } from "./SidebarNavigation";

it("marks only the active navigation target and preserves links and button actions", () => {
	const onClick = vi.fn();
	render(
		<MemoryRouter>
			<SidebarNavigation
				items={[
					{
						key: "files",
						icon: "Folder",
						label: "Files",
						to: "/",
						active: true,
					},
					{
						key: "trash",
						icon: "Trash",
						label: "Trash",
						to: "/trash",
						active: false,
					},
					{
						key: "inspected-trash",
						icon: "Trash",
						label: "Inspected trash",
						active: false,
						onClick,
					},
				]}
			/>
		</MemoryRouter>,
	);
	expect(screen.getByRole("link", { name: "Files" })).toHaveAttribute(
		"aria-current",
		"page",
	);
	expect(screen.getByRole("link", { name: "Trash" })).not.toHaveAttribute(
		"aria-current",
	);
	fireEvent.click(screen.getByRole("button", { name: "Inspected trash" }));
	expect(onClick).toHaveBeenCalledOnce();
});
