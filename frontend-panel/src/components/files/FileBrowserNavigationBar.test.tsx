import { fireEvent, render, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { beforeEach, expect, it, vi } from "vitest";
import { FileBrowserNavigationBar } from "./FileBrowserNavigationBar";

const state = vi.hoisted(() => ({ compact: false }));
vi.mock("@/hooks/useMediaQuery", () => ({
	useMediaQuery: () => state.compact,
}));
vi.mock("react-i18next", () => ({
	useTranslation: () => ({ t: (key: string) => key }),
}));
beforeEach(() => {
	state.compact = false;
});
function mount() {
	const refresh = vi.fn();
	const navigate = vi.fn();
	const viewMode = vi.fn();
	return {
		refresh,
		navigate,
		viewMode,
		...render(
			<FileBrowserNavigationBar
				icon="FolderOpen"
				breadcrumb={{
					items: [
						{ id: null, name: "Root" },
						{ id: 1, name: "Hidden ancestor" },
						{ id: 2, name: "Current" },
					],
					onNavigate: navigate,
				}}
				onRefresh={refresh}
				viewMode="list"
				onViewModeChange={viewMode}
			/>,
		),
	};
}
it("keeps the address flexible and refresh at its end with keyboard and view controls", async () => {
	const { refresh, viewMode } = mount();
	const breadcrumb = screen.getByRole("navigation");
	expect(breadcrumb.parentElement).toHaveClass("min-w-0", "flex-1");
	const button = screen.getByRole("button", { name: "core:refresh" });
	expect(breadcrumb.parentElement?.nextElementSibling).toBe(button);
	button.focus();
	await userEvent.keyboard("{Enter}");
	expect(refresh).toHaveBeenCalledOnce();
	fireEvent.click(screen.getByRole("button", { name: "grid_view" }));
	expect(viewMode).toHaveBeenCalledWith("grid");
	expect(
		screen.queryByRole("button", {
			name: /upload|new_file|tag_library|folder_more_actions/,
		}),
	).not.toBeInTheDocument();
});
it("collapses long paths on small screens and keeps hidden ancestors navigable", async () => {
	state.compact = true;
	const { navigate } = mount();
	expect(screen.queryByText("Hidden ancestor")).not.toBeInTheDocument();
	fireEvent.click(screen.getByRole("button", { name: "core:more" }));
	fireEvent.click(
		await screen.findByRole("menuitem", { name: "Hidden ancestor" }),
	);
	expect(navigate).toHaveBeenCalledExactlyOnceWith(1, "Hidden ancestor");
});
