import { fireEvent, render, screen, waitFor } from "@testing-library/react";
import type { ReactNode } from "react";
import { beforeEach, describe, expect, it, vi } from "vitest";
import { GlobalSearchDialog } from "@/components/layout/GlobalSearchDialog";
import type { TagInfo } from "@/types/api";

const mockState = vi.hoisted(() => ({
	listTags: vi.fn(),
	navigate: vi.fn(),
	workspace: { kind: "personal" as const },
}));

vi.mock("react-i18next", () => ({
	useTranslation: () => ({
		t: (key: string, options?: Record<string, unknown>) =>
			options ? `${key}:${JSON.stringify(options)}` : key,
	}),
}));

vi.mock("react-router-dom", () => ({
	useNavigate: () => mockState.navigate,
}));

vi.mock("@/stores/workspaceStore", () => ({
	useWorkspaceStore: (
		selector: (state: { workspace: typeof mockState.workspace }) => unknown,
	) => selector({ workspace: mockState.workspace }),
}));

vi.mock("@/services/tagService", () => ({
	createTagService: () => ({
		listTags: mockState.listTags,
	}),
}));

vi.mock("@/components/ui/dialog", () => ({
	Dialog: ({
		open,
		children,
	}: {
		open: boolean;
		onOpenChange: (open: boolean) => void;
		children: ReactNode;
	}) => (open ? <div>{children}</div> : null),
	DialogContent: ({ children }: { children: ReactNode }) => (
		<div>{children}</div>
	),
	DialogHeader: ({ children }: { children: ReactNode }) => (
		<div>{children}</div>
	),
	DialogTitle: ({ children }: { children: ReactNode }) => <div>{children}</div>,
}));

vi.mock("@/components/ui/icon", () => ({
	Icon: ({ name }: { name: string }) => (
		<span data-testid="icon" data-name={name} />
	),
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

describe("GlobalSearchDialog", () => {
	beforeEach(() => {
		mockState.listTags.mockReset();
		mockState.listTags.mockResolvedValue({
			items: [],
			limit: 100,
			offset: 0,
			total: 0,
		});
		mockState.navigate.mockReset();
	});

	it("submits keyword searches to the file-browser search route with Enter", () => {
		const onOpenChange = vi.fn();

		render(<GlobalSearchDialog open onOpenChange={onOpenChange} />);

		const input = screen.getByPlaceholderText("search:placeholder");
		fireEvent.change(input, {
			target: { value: "report" },
		});
		fireEvent.keyDown(input, { key: "Enter" });

		expect(onOpenChange).toHaveBeenCalledWith(false);
		expect(mockState.navigate).toHaveBeenCalledWith(
			"/search?q=report&type=all",
			{ viewTransition: false },
		);
	});

	it("submits searches from the explicit button and does not preview results inline", () => {
		const onOpenChange = vi.fn();

		render(<GlobalSearchDialog open onOpenChange={onOpenChange} />);

		expect(screen.getByText("search:dialog_empty")).toBeInTheDocument();
		const submitButton = screen.getByRole("button", {
			name: /search:submit_search/,
		});
		expect(submitButton).toBeDisabled();

		fireEvent.change(screen.getByPlaceholderText("search:placeholder"), {
			target: { value: "invoice" },
		});

		expect(screen.getByText("search:dialog_ready")).toBeInTheDocument();
		expect(screen.queryByText("search:results")).not.toBeInTheDocument();
		fireEvent.click(submitButton);

		expect(onOpenChange).toHaveBeenCalledWith(false);
		expect(mockState.navigate).toHaveBeenCalledWith(
			"/search?q=invoice&type=all",
			{ viewTransition: false },
		);
	});

	it("submits category filters without requiring a keyword", () => {
		const onOpenChange = vi.fn();

		render(<GlobalSearchDialog open onOpenChange={onOpenChange} />);

		fireEvent.click(
			screen.getByRole("button", { name: /search:show_filters/ }),
		);
		fireEvent.click(
			screen.getByRole("button", { name: "search:category_image" }),
		);
		fireEvent.click(
			screen.getByRole("button", { name: /search:submit_search/ }),
		);

		expect(onOpenChange).toHaveBeenCalledWith(false);
		expect(mockState.navigate).toHaveBeenCalledWith(
			"/search?type=file&category=image",
			{ viewTransition: false },
		);
	});

	it("submits tag filters without exposing tag library management", async () => {
		const onOpenChange = vi.fn();
		mockState.listTags.mockResolvedValueOnce({
			items: [tag(1, "Alpha"), tag(2, "Beta", "#16a34a")],
			limit: 100,
			offset: 0,
			total: 2,
		});

		render(<GlobalSearchDialog open onOpenChange={onOpenChange} />);

		fireEvent.click(
			screen.getByRole("button", { name: /search:show_filters/ }),
		);
		fireEvent.click(screen.getByRole("button", { name: /search:select_tags/ }));
		expect(
			await screen.findByRole("button", { name: /Alpha/ }),
		).toBeInTheDocument();
		fireEvent.click(screen.getByRole("button", { name: /Alpha/ }));
		fireEvent.click(screen.getByRole("button", { name: /Beta/ }));
		fireEvent.click(
			screen.getByRole("button", { name: "search:tag_match_any" }),
		);
		expect(
			screen.queryByRole("button", { name: /files:tag_library_manage/ }),
		).not.toBeInTheDocument();

		fireEvent.click(
			screen.getByRole("button", { name: /search:submit_search/ }),
		);

		expect(onOpenChange).toHaveBeenCalledWith(false);
		expect(mockState.navigate).toHaveBeenCalledWith(
			"/search?type=all&tag_ids=1%2C2&tag_match=all",
			{ viewTransition: false },
		);
	});

	it("keeps an empty tag filter list when tag loading fails or is canceled", async () => {
		mockState.listTags.mockRejectedValueOnce(new Error("tag load failed"));

		const { rerender } = render(
			<GlobalSearchDialog open onOpenChange={vi.fn()} />,
		);

		await waitFor(() => {
			expect(screen.queryByText("search:tag_loading")).not.toBeInTheDocument();
		});
		expect(
			screen.queryByRole("button", { name: /Alpha/ }),
		).not.toBeInTheDocument();

		const aborted = new DOMException("aborted", "AbortError");
		mockState.listTags.mockRejectedValueOnce(aborted);
		rerender(<GlobalSearchDialog open={false} onOpenChange={vi.fn()} />);
		rerender(<GlobalSearchDialog open onOpenChange={vi.fn()} />);

		await waitFor(() => {
			expect(screen.queryByText("search:tag_loading")).not.toBeInTheDocument();
		});
	});

	it("applies an initial category preset when opened from quick views", () => {
		const onOpenChange = vi.fn();

		render(
			<GlobalSearchDialog
				initialCategory="audio"
				open
				onOpenChange={onOpenChange}
			/>,
		);

		fireEvent.click(
			screen.getByRole("button", { name: /search:submit_search/ }),
		);

		expect(onOpenChange).toHaveBeenCalledWith(false);
		expect(mockState.navigate).toHaveBeenCalledWith(
			"/search?type=file&category=audio",
			{ viewTransition: false },
		);
	});

	it("leaves keyboard events to the IME while the search input is composing", () => {
		const onOpenChange = vi.fn();

		render(<GlobalSearchDialog open onOpenChange={onOpenChange} />);

		const input = screen.getByPlaceholderText("search:placeholder");
		fireEvent.change(input, {
			target: { value: "bao" },
		});
		fireEvent.compositionStart(input);
		fireEvent.keyDown(input, { key: "Enter" });
		fireEvent.keyDown(input, { key: "Escape" });

		expect(onOpenChange).not.toHaveBeenCalled();
		expect(mockState.navigate).not.toHaveBeenCalled();
	});

	it("closes on escape and resets stale input when reopened", () => {
		const onOpenChange = vi.fn();

		const { rerender } = render(
			<GlobalSearchDialog open onOpenChange={onOpenChange} />,
		);

		const input = screen.getByPlaceholderText("search:placeholder");
		fireEvent.change(input, {
			target: { value: "report" },
		});
		fireEvent.keyDown(input, { key: "Escape" });
		expect(onOpenChange).toHaveBeenCalledWith(false);

		rerender(<GlobalSearchDialog open={false} onOpenChange={onOpenChange} />);
		rerender(<GlobalSearchDialog open onOpenChange={onOpenChange} />);

		expect(screen.getByPlaceholderText("search:placeholder")).toHaveValue("");
		expect(screen.getByText("search:dialog_empty")).toBeInTheDocument();
	});

	it("handles header close, input blur, and composition end events", () => {
		const onOpenChange = vi.fn();

		render(<GlobalSearchDialog open onOpenChange={onOpenChange} />);

		const input = screen.getByPlaceholderText("search:placeholder");
		fireEvent.compositionStart(input);
		fireEvent.change(input, {
			target: { value: "report" },
		});
		fireEvent.compositionEnd(input);
		fireEvent.blur(input);

		const closeIcon = screen
			.getAllByTestId("icon")
			.find((icon) => icon.getAttribute("data-name") === "X");
		expect(closeIcon).toBeDefined();
		fireEvent.click(closeIcon?.closest("button") as HTMLButtonElement);

		expect(onOpenChange).toHaveBeenCalledWith(false);
	});
});
