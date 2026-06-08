import { fireEvent, render, screen } from "@testing-library/react";
import { createRef } from "react";
import { describe, expect, it, vi } from "vitest";
import { GlobalSearchHeader } from "@/components/layout/global-search/GlobalSearchHeader";
import type { TagInfo } from "@/types/api";

vi.mock("react-i18next", () => ({
	useTranslation: () => ({
		t: (key: string) => key,
	}),
}));

vi.mock("@/components/ui/button", () => ({
	Button: ({
		children,
		onClick,
		type,
		...props
	}: React.ButtonHTMLAttributes<HTMLButtonElement>) => (
		<button {...props} type={type ?? "button"} onClick={onClick}>
			{children}
		</button>
	),
}));

vi.mock("@/components/ui/icon", () => ({
	Icon: ({ name }: { name: string }) => <span aria-hidden>{name}</span>,
}));

vi.mock("@/components/ui/input", () => ({
	Input: ({
		ref,
		...props
	}: React.InputHTMLAttributes<HTMLInputElement> & {
		ref?: React.Ref<HTMLInputElement>;
	}) => <input {...props} ref={ref} />,
}));

function tag(id: number, name: string, color = "#2563eb"): TagInfo {
	return {
		id,
		name,
		color,
		usage_count: 0,
		scope_type: "personal",
		owner_user_id: 1,
		team_id: null,
		normalized_name: name.trim().toLowerCase(),
		sort_order: 0,
		created_at: "2026-06-08T00:00:00Z",
		updated_at: "2026-06-08T00:00:00Z",
	};
}

function renderHeader(
	overrides: Partial<React.ComponentProps<typeof GlobalSearchHeader>> = {},
) {
	const props = {
		categoryFilter: "all",
		filter: "all",
		inputRef: createRef<HTMLInputElement>(),
		onCategoryFilterChange: vi.fn(),
		onClose: vi.fn(),
		onFilterChange: vi.fn(),
		onInputBlur: vi.fn(),
		onInputCompositionEnd: vi.fn(),
		onInputCompositionStart: vi.fn(),
		onInputKeyDown: vi.fn(),
		onManageTagLibrary: vi.fn(),
		onQueryChange: vi.fn(),
		onTagMatchChange: vi.fn(),
		onTagToggle: vi.fn(),
		query: "report",
		selectedTagIds: [],
		tagLoading: false,
		tagMatch: "any",
		tags: [tag(1, "Important"), tag(2, "Archive", "not-a-color")],
		...overrides,
	} satisfies React.ComponentProps<typeof GlobalSearchHeader>;

	render(<GlobalSearchHeader {...props} />);

	return props;
}

describe("GlobalSearchHeader", () => {
	it("forwards input lifecycle events and closes from the header button", () => {
		const props = renderHeader();
		const input = screen.getByPlaceholderText("search:placeholder");

		fireEvent.change(input, { target: { value: "draft" } });
		fireEvent.compositionStart(input);
		fireEvent.compositionEnd(input, { currentTarget: { value: "draft" } });
		fireEvent.blur(input);
		fireEvent.keyDown(input, { key: "Enter" });
		fireEvent.click(screen.getByRole("button", { name: "" }));

		expect(props.onQueryChange).toHaveBeenCalledWith("draft");
		expect(props.onInputCompositionStart).toHaveBeenCalledTimes(1);
		expect(props.onInputCompositionEnd).toHaveBeenCalledWith("report");
		expect(props.onInputBlur).toHaveBeenCalledTimes(1);
		expect(props.onInputKeyDown).toHaveBeenCalledTimes(1);
		expect(props.onClose).toHaveBeenCalledTimes(1);
	});

	it("changes result type and category filters", () => {
		const props = renderHeader({
			categoryFilter: "image",
			filter: "file",
		});

		fireEvent.click(
			screen.getByRole("button", { name: "search:folders_only" }),
		);
		fireEvent.click(
			screen.getByRole("button", { name: "search:category_video" }),
		);

		expect(props.onFilterChange).toHaveBeenCalledWith("folder");
		expect(props.onCategoryFilterChange).toHaveBeenCalledWith("video");
		expect(
			screen.getByRole("button", { name: "search:category_image" }),
		).toHaveAttribute("aria-pressed", "true");
	});

	it("hides quick categories for folder-only searches", () => {
		renderHeader({ filter: "folder" });

		expect(
			screen.queryByRole("button", { name: "search:category_image" }),
		).not.toBeInTheDocument();
		expect(screen.getAllByText("search:tag_filters").length).toBeGreaterThan(0);
	});

	it("toggles tag filters and switches match mode when multiple tags are selected", () => {
		const props = renderHeader({
			selectedTagIds: [1, 2],
			tagMatch: "any",
		});

		fireEvent.click(screen.getByRole("button", { name: "Important" }));
		fireEvent.click(
			screen.getByRole("button", { name: "search:tag_match_any" }),
		);
		fireEvent.click(
			screen.getByRole("button", { name: "files:tag_library_manage" }),
		);

		expect(props.onTagToggle).toHaveBeenCalledWith(1);
		expect(props.onTagMatchChange).toHaveBeenCalledWith("all");
		expect(props.onManageTagLibrary).toHaveBeenCalledTimes(1);
		expect(screen.getByRole("button", { name: "Important" })).toHaveAttribute(
			"aria-pressed",
			"true",
		);
	});

	it("does not show tag match mode until at least two tags are selected", () => {
		renderHeader({ selectedTagIds: [1] });

		expect(
			screen.queryByRole("button", { name: "search:tag_match_any" }),
		).not.toBeInTheDocument();
	});

	it("shows tag loading state instead of tag chips", () => {
		renderHeader({ tagLoading: true });

		expect(screen.getByText("search:tag_loading")).toBeInTheDocument();
		expect(
			screen.queryByRole("button", { name: "Important" }),
		).not.toBeInTheDocument();
	});
});
