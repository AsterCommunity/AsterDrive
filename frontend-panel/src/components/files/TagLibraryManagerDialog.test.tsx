import {
	cleanup,
	fireEvent,
	render,
	screen,
	waitFor,
} from "@testing-library/react";
import { beforeEach, describe, expect, it, vi } from "vitest";
import { TagLibraryManagerDialog } from "@/components/files/TagLibraryManagerDialog";
import type { TagInfo } from "@/types/api";

const mockState = vi.hoisted(() => ({
	createTag: vi.fn(),
	deleteTag: vi.fn(),
	handleApiError: vi.fn(),
	listTags: vi.fn(),
	patchTag: vi.fn(),
	toastSuccess: vi.fn(),
}));

vi.mock("react-i18next", () => ({
	useTranslation: () => ({
		t: (key: string, options?: Record<string, unknown>) => {
			const normalizedKey = key.replace(/^core:/, "");
			if (normalizedKey === "tag_usage_count") {
				return `tag_usage_count:${options?.count}`;
			}
			if (normalizedKey === "tag_delete_confirm_desc") {
				return `tag_delete_confirm_desc:${options?.name}`;
			}
			if (normalizedKey === "tag_color_option") {
				return `tag_color_option:${options?.color}`;
			}
			if (normalizedKey === "tag_create_named") {
				return `create:${options?.name}`;
			}
			return normalizedKey;
		},
	}),
}));

vi.mock("sonner", () => ({
	toast: {
		success: (...args: unknown[]) => mockState.toastSuccess(...args),
	},
}));

vi.mock("@/components/ui/button", () => ({
	Button: ({
		children,
		disabled,
		onClick,
		type,
		...props
	}: React.ButtonHTMLAttributes<HTMLButtonElement>) => (
		<button
			{...props}
			type={type ?? "button"}
			disabled={disabled}
			onClick={onClick}
		>
			{children}
		</button>
	),
}));

vi.mock("@/components/ui/dialog", () => ({
	Dialog: ({ children, open }: { children: React.ReactNode; open: boolean }) =>
		open ? <div data-testid="dialog">{children}</div> : null,
	DialogContent: ({
		children,
		className,
	}: {
		children: React.ReactNode;
		className?: string;
	}) => (
		<div className={className} data-testid="dialog-content">
			{children}
		</div>
	),
	DialogDescription: ({ children }: { children: React.ReactNode }) => (
		<p>{children}</p>
	),
	DialogHeader: ({ children }: { children: React.ReactNode }) => (
		<div>{children}</div>
	),
	DialogTitle: ({ children }: { children: React.ReactNode }) => (
		<h2>{children}</h2>
	),
}));

vi.mock("@/components/ui/icon", () => ({
	Icon: ({ name }: { name: string }) => <span aria-hidden>{name}</span>,
}));

vi.mock("@/components/ui/input", () => ({
	Input: ({ ...props }: React.InputHTMLAttributes<HTMLInputElement>) => (
		<input {...props} />
	),
}));

vi.mock("@/hooks/useApiError", () => ({
	handleApiError: (...args: unknown[]) => mockState.handleApiError(...args),
}));

vi.mock("@/services/tagService", () => ({
	tagService: {
		createTag: (...args: unknown[]) => mockState.createTag(...args),
		deleteTag: (...args: unknown[]) => mockState.deleteTag(...args),
		listTags: (...args: unknown[]) => mockState.listTags(...args),
		patchTag: (...args: unknown[]) => mockState.patchTag(...args),
	},
}));

function tag(
	id: number,
	name: string,
	color = "#2563eb",
	usageCount = 0,
): TagInfo {
	return {
		id,
		name,
		color,
		usage_count: usageCount,
		scope_type: "personal",
		owner_user_id: 1,
		team_id: null,
		normalized_name: name.trim().toLowerCase(),
		sort_order: 0,
		created_at: "2026-06-08T00:00:00Z",
		updated_at: "2026-06-08T00:00:00Z",
	};
}

describe("TagLibraryManagerDialog", () => {
	const alpha = tag(1, "Alpha", "#2563eb", 2);
	const beta = tag(2, "Beta", "#16a34a", 0);
	const gamma = tag(3, "Gamma", "#dc2626", 1);

	beforeEach(() => {
		mockState.createTag.mockReset();
		mockState.deleteTag.mockReset();
		mockState.handleApiError.mockReset();
		mockState.listTags.mockReset();
		mockState.patchTag.mockReset();
		mockState.toastSuccess.mockReset();

		mockState.createTag.mockResolvedValue(gamma);
		mockState.deleteTag.mockResolvedValue(undefined);
		mockState.listTags.mockResolvedValue({
			items: [beta, alpha],
			total: 2,
		});
		mockState.patchTag.mockResolvedValue(tag(1, "Alpha Prime", "#2563eb", 2));
	});

	it("loads, sorts, searches, and loads more tags", async () => {
		mockState.listTags
			.mockResolvedValueOnce({ items: [beta], total: 2 })
			.mockResolvedValueOnce({ items: [alpha], total: 1 })
			.mockResolvedValueOnce({ items: [gamma], total: 3 });

		render(<TagLibraryManagerDialog open onOpenChange={vi.fn()} />);

		expect(await screen.findByText("Beta")).toBeInTheDocument();
		expect(mockState.listTags).toHaveBeenNthCalledWith(1, {
			params: { limit: 50, offset: 0 },
		});
		expect(
			screen.getByRole("button", { name: "tag_library_load_more" }),
		).toBeInTheDocument();

		fireEvent.change(screen.getByLabelText("tag_search_label"), {
			target: { value: " Alpha " },
		});

		await waitFor(() => {
			expect(mockState.listTags).toHaveBeenNthCalledWith(2, {
				params: { limit: 50, offset: 0, q: "Alpha" },
			});
		});
		expect(await screen.findByText("Alpha")).toBeInTheDocument();

		fireEvent.change(screen.getByLabelText("tag_search_label"), {
			target: { value: "" },
		});
		await waitFor(() => {
			expect(mockState.listTags).toHaveBeenNthCalledWith(3, {
				params: { limit: 50, offset: 0 },
			});
		});
		fireEvent.click(
			screen.getByRole("button", { name: "tag_library_load_more" }),
		);

		await waitFor(() => {
			expect(mockState.listTags).toHaveBeenNthCalledWith(4, {
				params: { limit: 50, offset: 1 },
			});
		});
	});

	it("keeps the tag list inside a fixed-height scroll area", async () => {
		render(<TagLibraryManagerDialog open onOpenChange={vi.fn()} />);

		expect(await screen.findByText("Alpha")).toBeInTheDocument();
		expect(screen.getByTestId("dialog-content")).toHaveClass(
			"sm:h-[min(88vh,44rem)]",
		);
		const scrollArea = screen.getByText("Alpha").closest(".overflow-y-auto");
		expect(scrollArea).toHaveClass("h-full", "overflow-y-auto");
	});

	it("creates a tag from the library search row and reports it to the parent", async () => {
		const onTagCreated = vi.fn();
		let includeCreatedTag = false;
		mockState.createTag.mockImplementation(async () => {
			includeCreatedTag = true;
			return gamma;
		});
		mockState.listTags.mockImplementation(() =>
			Promise.resolve({
				items: includeCreatedTag ? [beta, alpha, gamma] : [beta, alpha],
				total: includeCreatedTag ? 3 : 2,
			}),
		);

		render(
			<TagLibraryManagerDialog
				open
				onOpenChange={vi.fn()}
				onTagCreated={onTagCreated}
			/>,
		);

		await screen.findByText("Alpha");
		fireEvent.change(screen.getByLabelText("tag_search_label"), {
			target: { value: " Gamma " },
		});
		fireEvent.click(
			await screen.findByRole("button", { name: /create:Gamma/ }),
		);

		await waitFor(() => {
			expect(mockState.createTag).toHaveBeenCalledWith({
				name: "Gamma",
				color: expect.stringMatching(/^#[0-9a-f]{6}$/),
			});
		});
		expect(onTagCreated).toHaveBeenCalledWith(gamma);
		expect(mockState.toastSuccess).toHaveBeenCalledWith("tag_library_created");
		expect(await screen.findByText("Gamma")).toBeInTheDocument();
	});

	it("creates a tag with Enter from the library search field", async () => {
		render(<TagLibraryManagerDialog open onOpenChange={vi.fn()} />);

		await screen.findByText("Alpha");
		fireEvent.change(screen.getByLabelText("tag_search_label"), {
			target: { value: " Gamma " },
		});
		await screen.findByRole("button", { name: /create:Gamma/ });
		fireEvent.keyDown(screen.getByLabelText("tag_search_label"), {
			key: "Enter",
		});

		await waitFor(() => {
			expect(mockState.createTag).toHaveBeenCalledWith({
				name: "Gamma",
				color: expect.stringMatching(/^#[0-9a-f]{6}$/),
			});
		});
	});

	it("does not offer creation for blank names or duplicate names", async () => {
		render(<TagLibraryManagerDialog open onOpenChange={vi.fn()} />);

		await screen.findByText("Alpha");
		fireEvent.change(screen.getByLabelText("tag_search_label"), {
			target: { value: "   " },
		});
		expect(screen.queryByRole("button", { name: /create:/ })).toBeNull();
		fireEvent.keyDown(screen.getByLabelText("tag_search_label"), {
			key: "Enter",
		});

		fireEvent.change(screen.getByLabelText("tag_search_label"), {
			target: { value: " alpha " },
		});
		await waitFor(() => {
			expect(mockState.listTags).toHaveBeenLastCalledWith({
				params: { limit: 50, offset: 0, q: "alpha" },
			});
		});
		expect(screen.queryByRole("button", { name: /create:/ })).toBeNull();
		fireEvent.keyDown(screen.getByLabelText("tag_search_label"), {
			key: "Enter",
		});

		expect(mockState.createTag).not.toHaveBeenCalled();
	});

	it("renames a tag, trims the submitted name, and reports the update", async () => {
		const onTagUpdated = vi.fn();

		render(
			<TagLibraryManagerDialog
				open
				onOpenChange={vi.fn()}
				onTagUpdated={onTagUpdated}
			/>,
		);

		await screen.findByText("Alpha");
		fireEvent.click(screen.getAllByRole("button", { name: "tag_edit" })[0]);

		const input = screen.getByLabelText("tag_name");
		fireEvent.change(input, { target: { value: "  Alpha Prime  " } });
		fireEvent.click(screen.getByRole("button", { name: "save" }));

		await waitFor(() => {
			expect(mockState.patchTag).toHaveBeenCalledWith(1, {
				color: "#2563eb",
				name: "Alpha Prime",
			});
		});
		expect(onTagUpdated).toHaveBeenCalledWith(
			expect.objectContaining({ id: 1, name: "Alpha Prime" }),
		);
		expect(mockState.toastSuccess).toHaveBeenCalledWith("tag_updated");
		expect(await screen.findByText("Alpha Prime")).toBeInTheDocument();
		expect(screen.queryByLabelText("tag_name")).not.toBeInTheDocument();
	});

	it("updates a tag color from the inline editor", async () => {
		mockState.patchTag.mockResolvedValue(tag(1, "Alpha", "#dc2626", 2));
		const onTagUpdated = vi.fn();

		render(
			<TagLibraryManagerDialog
				open
				onOpenChange={vi.fn()}
				onTagUpdated={onTagUpdated}
			/>,
		);

		await screen.findByText("Alpha");
		fireEvent.click(screen.getAllByRole("button", { name: "tag_edit" })[0]);
		expect(screen.getByRole("button", { name: "save" })).toBeDisabled();

		fireEvent.click(
			screen.getByRole("button", { name: "tag_color_option:#dc2626" }),
		);
		fireEvent.click(screen.getByRole("button", { name: "save" }));

		await waitFor(() => {
			expect(mockState.patchTag).toHaveBeenCalledWith(1, {
				color: "#dc2626",
				name: "Alpha",
			});
		});
		expect(onTagUpdated).toHaveBeenCalledWith(
			expect.objectContaining({ color: "#dc2626", id: 1, name: "Alpha" }),
		);
		expect(mockState.toastSuccess).toHaveBeenCalledWith("tag_updated");
	});

	it("cancels edit with Escape and keeps unchanged names disabled", async () => {
		render(<TagLibraryManagerDialog open onOpenChange={vi.fn()} />);

		await screen.findByText("Alpha");
		fireEvent.click(screen.getAllByRole("button", { name: "tag_edit" })[0]);

		expect(screen.getByRole("button", { name: "save" })).toBeDisabled();
		fireEvent.change(screen.getByLabelText("tag_name"), {
			target: { value: "   " },
		});
		expect(screen.getByRole("button", { name: "save" })).toBeDisabled();

		fireEvent.keyDown(screen.getByLabelText("tag_name"), { key: "Escape" });

		expect(screen.queryByLabelText("tag_name")).not.toBeInTheDocument();
		expect(mockState.patchTag).not.toHaveBeenCalled();
	});

	it("saves edits with Enter and cancels delete confirmation", async () => {
		render(<TagLibraryManagerDialog open onOpenChange={vi.fn()} />);

		await screen.findByText("Alpha");
		fireEvent.click(screen.getAllByRole("button", { name: "tag_edit" })[0]);
		fireEvent.change(screen.getByLabelText("tag_name"), {
			target: { value: "Alpha Prime" },
		});
		fireEvent.keyDown(screen.getByLabelText("tag_name"), { key: "Enter" });

		await waitFor(() => {
			expect(mockState.patchTag).toHaveBeenCalledWith(1, {
				color: "#2563eb",
				name: "Alpha Prime",
			});
		});

		fireEvent.click(screen.getAllByRole("button", { name: "tag_delete" })[0]);
		expect(
			screen.getByText("tag_delete_confirm_desc:Alpha Prime"),
		).toBeInTheDocument();
		fireEvent.click(screen.getByRole("button", { name: "cancel" }));

		expect(
			screen.queryByText("tag_delete_confirm_desc:Alpha Prime"),
		).not.toBeInTheDocument();
		expect(mockState.deleteTag).not.toHaveBeenCalled();
	});

	it("renders empty states for an empty library and empty searches", async () => {
		mockState.listTags
			.mockResolvedValueOnce({ items: [], total: 0 })
			.mockResolvedValueOnce({ items: [], total: 0 });

		render(<TagLibraryManagerDialog open onOpenChange={vi.fn()} />);

		expect(await screen.findByText("tag_library_empty")).toBeInTheDocument();

		fireEvent.change(screen.getByLabelText("tag_search_label"), {
			target: { value: "missing" },
		});

		await waitFor(() => {
			expect(mockState.listTags).toHaveBeenNthCalledWith(2, {
				params: { limit: 50, offset: 0, q: "missing" },
			});
		});
		expect(await screen.findByText("tag_search_empty")).toBeInTheDocument();
	});

	it("confirms deletion and updates local state", async () => {
		const onTagDeleted = vi.fn();

		render(
			<TagLibraryManagerDialog
				open
				onOpenChange={vi.fn()}
				onTagDeleted={onTagDeleted}
			/>,
		);

		await screen.findByText("Alpha");
		fireEvent.click(screen.getAllByRole("button", { name: "tag_delete" })[0]);
		expect(screen.getByText("tag_delete_confirm_title")).toBeInTheDocument();
		expect(
			screen.getByText("tag_delete_confirm_desc:Alpha"),
		).toBeInTheDocument();
		expect(
			screen.queryByText("tag_delete_confirm_desc:Beta"),
		).not.toBeInTheDocument();

		fireEvent.click(screen.getByRole("button", { name: "delete" }));

		await waitFor(() => {
			expect(mockState.deleteTag).toHaveBeenCalledWith(1);
		});
		expect(onTagDeleted).toHaveBeenCalledWith(1);
		expect(mockState.toastSuccess).toHaveBeenCalledWith("tag_deleted");
		expect(screen.queryByText("Alpha")).not.toBeInTheDocument();
	});

	it("routes create failures through handleApiError and keeps the draft", async () => {
		const createError = new Error("create failed");
		mockState.createTag.mockRejectedValueOnce(createError);

		render(<TagLibraryManagerDialog open onOpenChange={vi.fn()} />);

		await screen.findByText("Alpha");
		fireEvent.change(screen.getByLabelText("tag_search_label"), {
			target: { value: "Delta" },
		});
		fireEvent.click(
			await screen.findByRole("button", { name: /create:Delta/ }),
		);

		await waitFor(() => {
			expect(mockState.handleApiError).toHaveBeenCalledWith(createError);
		});
		expect(screen.getByLabelText("tag_search_label")).toHaveValue("Delta");
		expect(mockState.toastSuccess).not.toHaveBeenCalledWith(
			"tag_library_created",
		);
	});

	it("routes load, edit, and delete failures through handleApiError", async () => {
		const loadError = new Error("load failed");
		const editError = new Error("edit failed");
		const deleteError = new Error("delete failed");
		mockState.listTags.mockRejectedValueOnce(loadError);

		render(<TagLibraryManagerDialog open onOpenChange={vi.fn()} />);

		await waitFor(() => {
			expect(mockState.handleApiError).toHaveBeenCalledWith(loadError);
		});

		cleanup();
		mockState.listTags.mockResolvedValueOnce({ items: [alpha], total: 1 });
		mockState.patchTag.mockRejectedValueOnce(editError);
		render(<TagLibraryManagerDialog open onOpenChange={vi.fn()} />);
		await screen.findByText("Alpha");
		fireEvent.click(screen.getByRole("button", { name: "tag_edit" }));
		fireEvent.change(screen.getByLabelText("tag_name"), {
			target: { value: "Alpha Prime" },
		});
		fireEvent.click(screen.getByRole("button", { name: "save" }));

		await waitFor(() => {
			expect(mockState.handleApiError).toHaveBeenCalledWith(editError);
		});
		expect(screen.getByLabelText("tag_name")).toBeInTheDocument();

		fireEvent.click(screen.getByRole("button", { name: "cancel" }));
		mockState.deleteTag.mockRejectedValueOnce(deleteError);
		fireEvent.click(screen.getByRole("button", { name: "tag_delete" }));
		fireEvent.click(screen.getByRole("button", { name: "delete" }));

		await waitFor(() => {
			expect(mockState.handleApiError).toHaveBeenCalledWith(deleteError);
		});
		expect(
			screen.getByText("tag_delete_confirm_desc:Alpha"),
		).toBeInTheDocument();
		expect(mockState.toastSuccess).not.toHaveBeenCalledWith("tag_deleted");
	});
});
