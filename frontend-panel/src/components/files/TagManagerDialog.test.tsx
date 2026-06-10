import { fireEvent, render, screen, waitFor } from "@testing-library/react";
import { beforeEach, describe, expect, it, vi } from "vitest";
import { TagManagerDialog } from "@/components/files/TagManagerDialog";
import type { TagInfo, TagSummary } from "@/types/api";

const mockState = vi.hoisted(() => ({
	batchAttachTag: vi.fn(),
	batchDetachTag: vi.fn(),
	createTag: vi.fn(),
	handleApiError: vi.fn(),
	keepDialogMountedWhenClosed: false,
	listTags: vi.fn(),
	replaceEntityTags: vi.fn(),
	toastSuccess: vi.fn(),
}));

vi.mock("react-i18next", () => ({
	useTranslation: () => ({
		t: (key: string, options?: Record<string, unknown>) => {
			const normalizedKey = key.replace(/^core:/, "");
			if (normalizedKey === "tag_create_named") {
				return `create:${options?.name}`;
			}
			if (normalizedKey === "tag_manage_batch_title") {
				return `tag_manage_batch_title:${options?.count}`;
			}
			if (normalizedKey === "tag_draft_batch_summary") {
				return `draft:add=${options?.add}:remove=${options?.remove}`;
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

vi.mock("@/components/files/TagLibraryManagerDialog", () => ({
	TagLibraryManagerDialog: ({
		onTagCreated,
		onTagDeleted,
		onTagUpdated,
		open,
	}: {
		open: boolean;
		onOpenChange: (open: boolean) => void;
		onTagCreated?: (tag: TagInfo) => void;
		onTagDeleted?: (tagId: number) => void;
		onTagUpdated?: (tag: TagInfo) => void;
	}) =>
		open ? (
			<div data-testid="tag-library-manager">
				<button
					type="button"
					onClick={() =>
						onTagCreated?.({
							id: 4,
							name: "Delta",
							color: "#0891b2",
							usage_count: 0,
							scope_type: "personal",
							owner_user_id: 1,
							team_id: null,
							normalized_name: "delta",
							sort_order: 0,
							created_at: "2026-06-08T00:00:00Z",
							updated_at: "2026-06-08T00:00:00Z",
						})
					}
				>
					library-create-delta
				</button>
				<button
					type="button"
					onClick={() =>
						onTagUpdated?.({
							id: 1,
							name: "Alpha Prime",
							color: "#7c3aed",
							usage_count: 2,
							scope_type: "personal",
							owner_user_id: 1,
							team_id: null,
							normalized_name: "alpha prime",
							sort_order: 0,
							created_at: "2026-06-08T00:00:00Z",
							updated_at: "2026-06-08T00:00:00Z",
						})
					}
				>
					library-update-alpha
				</button>
				<button type="button" onClick={() => onTagDeleted?.(1)}>
					library-delete-alpha
				</button>
			</div>
		) : null,
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
		open || mockState.keepDialogMountedWhenClosed ? (
			<div data-open={open} data-testid="dialog">
				{children}
			</div>
		) : null,
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
		batchAttachTag: (...args: unknown[]) => mockState.batchAttachTag(...args),
		batchDetachTag: (...args: unknown[]) => mockState.batchDetachTag(...args),
		createTag: (...args: unknown[]) => mockState.createTag(...args),
		listTags: (...args: unknown[]) => mockState.listTags(...args),
		replaceEntityTags: (...args: unknown[]) =>
			mockState.replaceEntityTags(...args),
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

function summary(tagInfo: TagInfo): TagSummary {
	return {
		id: tagInfo.id,
		name: tagInfo.name,
		color: tagInfo.color,
	};
}

describe("TagManagerDialog", () => {
	const alpha = tag(1, "Alpha", "#2563eb", 2);
	const beta = tag(2, "Beta", "#16a34a", 0);
	const gamma = tag(3, "Gamma", "#dc2626", 1);

	beforeEach(() => {
		mockState.batchAttachTag.mockReset();
		mockState.batchDetachTag.mockReset();
		mockState.createTag.mockReset();
		mockState.handleApiError.mockReset();
		mockState.keepDialogMountedWhenClosed = false;
		mockState.listTags.mockReset();
		mockState.replaceEntityTags.mockReset();
		mockState.toastSuccess.mockReset();

		mockState.batchAttachTag.mockResolvedValue(undefined);
		mockState.batchDetachTag.mockResolvedValue(undefined);
		mockState.createTag.mockResolvedValue(gamma);
		mockState.listTags.mockImplementation(
			({ params }: { params: { q?: string } }) => {
				const items = [beta, alpha].filter((item) =>
					params.q
						? item.name.toLowerCase().includes(params.q.trim().toLowerCase())
						: true,
				);
				return Promise.resolve({ items, total: items.length });
			},
		);
		mockState.replaceEntityTags.mockResolvedValue({ tags: [] });
	});

	it("loads the tag library and saves entity tag replacements", async () => {
		const onChanged = vi.fn().mockResolvedValue(undefined);
		const onOpenChange = vi.fn();
		const onTagsChange = vi.fn();

		render(
			<TagManagerDialog
				open
				onOpenChange={onOpenChange}
				target={{
					mode: "entity",
					entityType: "file",
					entityId: 42,
					initialTags: [summary(alpha)],
					name: "report.pdf",
					onChanged,
					onTagsChange,
				}}
			/>,
		);

		await screen.findByText("Beta");
		expect(mockState.listTags).toHaveBeenCalledWith({
			params: { limit: 100, offset: 0 },
		});

		fireEvent.click(screen.getByRole("button", { name: /Beta/ }));
		fireEvent.click(screen.getByRole("button", { name: "save" }));

		await waitFor(() => {
			expect(mockState.replaceEntityTags).toHaveBeenCalledWith(
				"file",
				42,
				[1, 2],
			);
		});
		expect(onTagsChange).toHaveBeenCalledWith([summary(alpha), summary(beta)]);
		expect(onChanged).toHaveBeenCalledTimes(1);
		expect(mockState.toastSuccess).toHaveBeenCalledWith("tag_saved");
		expect(onOpenChange).toHaveBeenCalledWith(false);
	});

	it("keeps the library list in its own scroll region above the footer", async () => {
		render(
			<TagManagerDialog
				open
				onOpenChange={vi.fn()}
				target={{
					mode: "entity",
					entityType: "file",
					entityId: 42,
					initialTags: [summary(alpha)],
				}}
			/>,
		);

		await screen.findByText("Alpha");
		expect(screen.getByTestId("dialog-content")).toHaveClass(
			"sm:h-[min(88vh,44rem)]",
		);
		expect(screen.getByRole("region", { name: "tag_library" })).toHaveClass(
			"min-h-0",
			"flex-1",
			"overflow-y-auto",
		);
	});

	it("creates a new tag and selects it for the entity draft", async () => {
		const onOpenChange = vi.fn();

		render(
			<TagManagerDialog
				open
				onOpenChange={onOpenChange}
				target={{
					mode: "entity",
					entityType: "folder",
					entityId: 9,
					initialTags: [],
				}}
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

		fireEvent.click(screen.getByRole("button", { name: "save" }));

		await waitFor(() => {
			expect(mockState.replaceEntityTags).toHaveBeenCalledWith(
				"folder",
				9,
				[3],
			);
		});
		expect(mockState.toastSuccess).toHaveBeenCalledWith("tag_created");
		expect(mockState.toastSuccess).toHaveBeenCalledWith("tag_saved");
		expect(onOpenChange).toHaveBeenCalledWith(false);
	});

	it("syncs tags created from the nested library manager back into the entity draft", async () => {
		render(
			<TagManagerDialog
				open
				onOpenChange={vi.fn()}
				target={{
					mode: "entity",
					entityType: "folder",
					entityId: 9,
					initialTags: [],
				}}
			/>,
		);

		await screen.findByText("Alpha");
		fireEvent.click(screen.getByRole("button", { name: "tag_library_manage" }));
		fireEvent.click(screen.getByText("library-create-delta"));
		fireEvent.click(await screen.findByRole("button", { name: /Delta/ }));
		fireEvent.click(screen.getByRole("button", { name: "save" }));

		await waitFor(() => {
			expect(mockState.replaceEntityTags).toHaveBeenCalledWith(
				"folder",
				9,
				[4],
			);
		});
	});

	it("creates a new tag from Enter and queues it for batch save", async () => {
		render(
			<TagManagerDialog
				open
				onOpenChange={vi.fn()}
				target={{
					mode: "batch",
					count: 2,
					fileIds: [10],
					folderIds: [20],
				}}
			/>,
		);

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
		expect(await screen.findByText("draft:add=1:remove=0")).toBeInTheDocument();

		fireEvent.click(screen.getByRole("button", { name: "save" }));

		await waitFor(() => {
			expect(mockState.batchAttachTag).toHaveBeenCalledWith(3, {
				file_ids: [10],
				folder_ids: [20],
			});
		});
	});

	it("loads more tags from the current offset when more library rows exist", async () => {
		mockState.listTags
			.mockResolvedValueOnce({ items: [alpha], total: 2 })
			.mockResolvedValueOnce({ items: [beta], total: 2 });

		render(
			<TagManagerDialog
				open
				onOpenChange={vi.fn()}
				target={{
					mode: "entity",
					entityType: "file",
					entityId: 1,
					initialTags: [],
				}}
			/>,
		);

		await screen.findByText("Alpha");
		fireEvent.click(
			screen.getByRole("button", { name: "tag_library_load_more" }),
		);

		await waitFor(() => {
			expect(mockState.listTags).toHaveBeenNthCalledWith(2, {
				params: { limit: 100, offset: 1 },
			});
		});
		expect(await screen.findByText("Beta")).toBeInTheDocument();
	});

	it("saves batch add and remove actions against the selected entities", async () => {
		render(
			<TagManagerDialog
				open
				onOpenChange={vi.fn()}
				target={{
					mode: "batch",
					count: 3,
					fileIds: [10, 11],
					folderIds: [20],
				}}
			/>,
		);

		await screen.findByText("Alpha");
		fireEvent.click(screen.getByRole("button", { name: /Beta/ }));
		expect(await screen.findByText("draft:add=1:remove=0")).toBeInTheDocument();
		fireEvent.click(screen.getAllByRole("button", { name: /tag_remove/ })[1]);
		expect(await screen.findByText("draft:add=1:remove=1")).toBeInTheDocument();

		fireEvent.click(screen.getByRole("button", { name: "save" }));

		await waitFor(() => {
			expect(mockState.batchAttachTag).toHaveBeenCalledWith(2, {
				file_ids: [10, 11],
				folder_ids: [20],
			});
		});
		expect(mockState.batchDetachTag).toHaveBeenCalledWith(1, {
			file_ids: [10, 11],
			folder_ids: [20],
		});
		expect(mockState.toastSuccess).toHaveBeenCalledWith("tag_batch_saved");
	});

	it("routes load and save failures through handleApiError without closing", async () => {
		const loadError = new Error("load failed");
		const saveError = new Error("save failed");
		const onOpenChange = vi.fn();
		mockState.listTags.mockRejectedValueOnce(loadError);

		const { rerender } = render(
			<TagManagerDialog
				open
				onOpenChange={onOpenChange}
				target={{
					mode: "entity",
					entityType: "file",
					entityId: 1,
					initialTags: [],
				}}
			/>,
		);

		await waitFor(() => {
			expect(mockState.handleApiError).toHaveBeenCalledWith(loadError);
		});

		mockState.listTags.mockResolvedValueOnce({ items: [alpha], total: 1 });
		mockState.replaceEntityTags.mockRejectedValueOnce(saveError);
		rerender(
			<TagManagerDialog
				open
				onOpenChange={onOpenChange}
				target={{
					mode: "entity",
					entityType: "file",
					entityId: 1,
					initialTags: [],
				}}
			/>,
		);

		await screen.findByText("Alpha");
		fireEvent.click(screen.getByRole("button", { name: /Alpha/ }));
		fireEvent.click(screen.getByRole("button", { name: "save" }));

		await waitFor(() => {
			expect(mockState.handleApiError).toHaveBeenCalledWith(saveError);
		});
		expect(onOpenChange).not.toHaveBeenCalledWith(false);
	});

	it("syncs library edits and deletions into the entity draft", async () => {
		render(
			<TagManagerDialog
				open
				onOpenChange={vi.fn()}
				target={{
					mode: "entity",
					entityType: "file",
					entityId: 42,
					initialTags: [summary(alpha)],
					name: "report.pdf",
				}}
			/>,
		);

		await screen.findByText("Alpha");
		fireEvent.click(screen.getByRole("button", { name: "tag_library_manage" }));
		fireEvent.click(screen.getByText("library-update-alpha"));

		await waitFor(() => {
			expect(screen.getAllByText("Alpha Prime")).toHaveLength(2);
		});
		expect(screen.queryByText("Alpha")).not.toBeInTheDocument();

		fireEvent.click(screen.getByText("library-delete-alpha"));

		await waitFor(() => {
			expect(screen.queryByText("Alpha Prime")).not.toBeInTheDocument();
		});
		expect(screen.getByText("tag_no_tags")).toBeInTheDocument();
	});

	it("removes deleted library tags from pending batch actions", async () => {
		render(
			<TagManagerDialog
				open
				onOpenChange={vi.fn()}
				target={{
					mode: "batch",
					count: 2,
					fileIds: [10],
					folderIds: [20],
				}}
			/>,
		);

		await screen.findByText("Alpha");
		fireEvent.click(screen.getByRole("button", { name: /Alpha/ }));
		expect(await screen.findByText("draft:add=1:remove=0")).toBeInTheDocument();

		fireEvent.click(screen.getByRole("button", { name: "tag_library_manage" }));
		fireEvent.click(screen.getByText("library-delete-alpha"));

		expect(await screen.findByText("tag_draft_empty")).toBeInTheDocument();
		expect(screen.getByRole("button", { name: "save" })).toBeDisabled();
	});

	it("resets drafts, query, and nested dialogs when closed", async () => {
		const target = {
			mode: "entity" as const,
			entityType: "file" as const,
			entityId: 42,
			initialTags: [summary(alpha)],
			name: "report.pdf",
		};

		const { rerender } = render(
			<TagManagerDialog open onOpenChange={vi.fn()} target={target} />,
		);

		await screen.findByText("Beta");
		fireEvent.click(screen.getByRole("button", { name: /Beta/ }));
		expect(screen.getByText("tag_draft_entity_summary")).toBeInTheDocument();
		fireEvent.change(screen.getByLabelText("tag_search_label"), {
			target: { value: "Gamma" },
		});
		fireEvent.click(screen.getByRole("button", { name: "tag_library_manage" }));
		expect(screen.getByTestId("tag-library-manager")).toBeInTheDocument();

		rerender(
			<TagManagerDialog open={false} onOpenChange={vi.fn()} target={null} />,
		);
		rerender(<TagManagerDialog open onOpenChange={vi.fn()} target={target} />);

		await screen.findByText("Beta");
		expect(screen.queryByTestId("tag-library-manager")).not.toBeInTheDocument();
		expect(screen.getByLabelText("tag_search_label")).toHaveValue("");
		expect(screen.getByText("tag_draft_empty")).toBeInTheDocument();
		expect(screen.getByRole("button", { name: "save" })).toBeDisabled();
	});

	it("keeps the existing draft state visible during the close animation", async () => {
		mockState.keepDialogMountedWhenClosed = true;
		const target = {
			mode: "entity" as const,
			entityType: "file" as const,
			entityId: 42,
			initialTags: [summary(alpha)],
			name: "report.pdf",
		};

		const { rerender } = render(
			<TagManagerDialog open onOpenChange={vi.fn()} target={target} />,
		);

		await screen.findByText("Alpha");
		expect(screen.getByText("tag_draft_empty")).toBeInTheDocument();

		rerender(
			<TagManagerDialog open={false} onOpenChange={vi.fn()} target={target} />,
		);

		expect(screen.getByTestId("dialog")).toHaveAttribute("data-open", "false");
		expect(screen.getByText("tag_draft_empty")).toBeInTheDocument();
		expect(screen.queryByText("tag_draft_entity_summary")).toBeNull();
	});

	it("routes create failures through handleApiError", async () => {
		const createError = new Error("create failed");
		mockState.listTags
			.mockResolvedValueOnce({ items: [alpha], total: 1 })
			.mockResolvedValueOnce({ items: [], total: 0 });
		mockState.createTag.mockRejectedValueOnce(createError);

		render(
			<TagManagerDialog
				open
				onOpenChange={vi.fn()}
				target={{
					mode: "entity",
					entityType: "file",
					entityId: 1,
					initialTags: [],
				}}
			/>,
		);

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
		expect(screen.getByRole("button", { name: "save" })).toBeDisabled();
	});

	it("routes load-more failures through handleApiError", async () => {
		const loadMoreError = new Error("load more failed");
		mockState.listTags
			.mockResolvedValueOnce({ items: [alpha], total: 2 })
			.mockRejectedValueOnce(loadMoreError);

		render(
			<TagManagerDialog
				open
				onOpenChange={vi.fn()}
				target={{
					mode: "entity",
					entityType: "file",
					entityId: 1,
					initialTags: [],
				}}
			/>,
		);

		await screen.findByText("Alpha");
		fireEvent.click(
			screen.getByRole("button", { name: "tag_library_load_more" }),
		);

		await waitFor(() => {
			expect(mockState.handleApiError).toHaveBeenCalledWith(loadMoreError);
		});
	});
});
