import { fireEvent, render, screen, waitFor } from "@testing-library/react";
import { beforeEach, describe, expect, it, vi } from "vitest";
import { BatchTargetFolderDialog } from "@/components/files/BatchTargetFolderDialog";
import { FOLDER_LIMIT } from "@/lib/constants";

const mockState = vi.hoisted(() => ({
	createFolder: vi.fn(),
	currentWorkspace: { kind: "personal" as const },
	handleApiError: vi.fn(),
	ensureTeamsLoaded: vi.fn(),
	listFolder: vi.fn(),
	listRoot: vi.fn(),
	teams: [] as Array<{ id: number; name: string }>,
	teamsLoading: false,
	translate: (key: string, opts?: Record<string, unknown>) => {
		if (key === "files:root") return "Root";
		if (key === "files:batch_move") return "batch-move";
		if (key === "files:batch_copy") return "batch-copy";
		if (key === "files:move_to_current_folder") return "move-here";
		if (key === "files:copy_to_current_folder") return "copy-here";
		if (key === "files:batch_target_folder_desc") return "target-desc";
		if (key === "files:batch_target_workspace") return "target-workspace";
		if (key === "files:batch_target_current_workspace") {
			return `workspace:${opts?.name}`;
		}
		if (key === "files:create_folder") return "create-folder";
		if (key === "files:folder_name") return "folder-name";
		if (key === "files:processing") return "processing";
		if (key === "files:batch_target_empty") return "empty";
		if (key === "files:batch_target_empty_desc") return "empty-desc";
		if (key === "files:batch_target_back") return "back";
		if (key === "files:batch_target_invalid_descendant") {
			return "invalid-descendant";
		}
		if (key === "files:create_folder_success") return "folder-created";
		if (key === "files:batch_target_current_folder") {
			return `current:${opts?.name}`;
		}
		if (key === "core:my_drive") return "My Drive";
		if (key === "core:workspace_personal_label") return "Personal";
		if (key === "core:workspace_team_label") return "Team";
		if (key === "cancel") return "cancel";
		return key;
	},
	toastSuccess: vi.fn(),
}));

vi.mock("react-i18next", () => ({
	useTranslation: () => ({
		t: mockState.translate,
	}),
}));

vi.mock("sonner", () => ({
	toast: {
		success: (...args: unknown[]) => mockState.toastSuccess(...args),
	},
}));

vi.mock("@/components/common/SkeletonTree", () => ({
	SkeletonTree: ({ count }: { count?: number }) => (
		<div>{`skeleton:${count ?? 5}`}</div>
	),
}));

vi.mock("@/components/ui/breadcrumb", () => ({
	Breadcrumb: ({ children }: { children: React.ReactNode }) => (
		<nav>{children}</nav>
	),
	BreadcrumbList: ({ children }: { children: React.ReactNode }) => (
		<div>{children}</div>
	),
	BreadcrumbItem: ({ children }: { children: React.ReactNode }) => (
		<span>{children}</span>
	),
	BreadcrumbLink: ({
		children,
		onClick,
		className,
	}: {
		children: React.ReactNode;
		onClick?: () => void;
		className?: string;
	}) => (
		<button type="button" onClick={onClick} className={className}>
			{children}
		</button>
	),
	BreadcrumbSeparator: () => <span>/</span>,
}));

vi.mock("@/components/ui/button", () => ({
	Button: ({
		children,
		disabled,
		onClick,
		className,
	}: {
		children: React.ReactNode;
		disabled?: boolean;
		onClick?: () => void;
		className?: string;
	}) => (
		<button
			type="button"
			disabled={disabled}
			onClick={onClick}
			className={className}
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
	}) => <div className={className}>{children}</div>,
	DialogHeader: ({
		children,
		className,
	}: {
		children: React.ReactNode;
		className?: string;
	}) => <div className={className}>{children}</div>,
	DialogTitle: ({ children }: { children: React.ReactNode }) => (
		<h2>{children}</h2>
	),
	DialogDescription: ({ children }: { children: React.ReactNode }) => (
		<p>{children}</p>
	),
	DialogFooter: ({
		children,
		className,
	}: {
		children: React.ReactNode;
		className?: string;
	}) => <div className={className}>{children}</div>,
}));

vi.mock("@/components/ui/icon", () => ({
	Icon: ({ name }: { name: string }) => (
		<span aria-hidden="true" data-name={name} />
	),
}));

vi.mock("@/components/ui/input", () => ({
	Input: ({ ...props }: React.InputHTMLAttributes<HTMLInputElement>) => (
		<input {...props} />
	),
}));

vi.mock("@/components/ui/select", () => ({
	Select: ({
		items,
		onValueChange,
		value,
	}: {
		children: React.ReactNode;
		items: Array<{ label: string; value: string }>;
		onValueChange?: (value: string) => void;
		value?: string;
	}) => (
		<select
			aria-label="target-workspace"
			value={value}
			onChange={(event) => onValueChange?.(event.currentTarget.value)}
		>
			{items.map((item) => (
				<option key={item.value} value={item.value}>
					{item.label}
				</option>
			))}
		</select>
	),
	SelectContent: ({ children }: { children: React.ReactNode }) => (
		<div>{children}</div>
	),
	SelectItem: ({
		children,
		value,
	}: {
		children: React.ReactNode;
		value: string;
	}) => <div data-value={value}>{children}</div>,
	SelectTrigger: ({ children }: { children: React.ReactNode }) => (
		<div>{children}</div>
	),
	SelectValue: ({ children }: { children: React.ReactNode }) => (
		<div>{children}</div>
	),
}));

vi.mock("@/components/ui/scroll-area", () => ({
	ScrollArea: ({
		children,
		className,
	}: {
		children: React.ReactNode;
		className?: string;
	}) => <div className={className}>{children}</div>,
}));

vi.mock("@/hooks/useApiError", () => ({
	handleApiError: (...args: unknown[]) => mockState.handleApiError(...args),
}));

vi.mock("@/services/fileService", () => ({
	createFileService: (workspace: unknown) => ({
		createFolder: (...args: unknown[]) =>
			mockState.createFolder(workspace, ...args),
		listFolder: (...args: unknown[]) =>
			mockState.listFolder(workspace, ...args),
		listRoot: (...args: unknown[]) => mockState.listRoot(workspace, ...args),
	}),
}));

vi.mock("@/stores/authStore", () => ({
	useAuthStore: (
		selector: (state: { user: { id: number } | null }) => unknown,
	) => selector({ user: { id: 42 } }),
}));

vi.mock("@/stores/teamStore", () => ({
	useTeamStore: (
		selector: (state: {
			teams: Array<{ id: number; name: string }>;
			loading: boolean;
			ensureLoaded: (userId: number | null) => Promise<void>;
		}) => unknown,
	) =>
		selector({
			teams: mockState.teams,
			loading: mockState.teamsLoading,
			ensureLoaded: mockState.ensureTeamsLoaded,
		}),
}));

vi.mock("@/stores/workspaceStore", () => ({
	useWorkspaceStore: (
		selector: (state: {
			workspace: { kind: "personal" } | { kind: "team"; teamId: number };
		}) => unknown,
	) => selector({ workspace: mockState.currentWorkspace }),
}));

const rootFolder = {
	id: 1,
	name: "Projects",
} as never;

const childFolder = {
	id: 2,
	name: "Design",
} as never;

const draftsFolder = {
	id: 6,
	name: "Drafts",
} as never;

function renderDialog(
	overrides: Partial<React.ComponentProps<typeof BatchTargetFolderDialog>> = {},
) {
	const onConfirm = vi.fn().mockResolvedValue(undefined);
	const onOpenChange = vi.fn();

	render(
		<BatchTargetFolderDialog
			open
			onOpenChange={onOpenChange}
			mode="move"
			onConfirm={onConfirm}
			currentFolderId={null}
			initialBreadcrumb={[]}
			selectedFolderIds={[]}
			{...overrides}
		/>,
	);

	return { onConfirm, onOpenChange };
}

describe("BatchTargetFolderDialog", () => {
	beforeEach(() => {
		mockState.createFolder.mockReset();
		mockState.currentWorkspace = { kind: "personal" };
		mockState.ensureTeamsLoaded.mockReset();
		mockState.ensureTeamsLoaded.mockResolvedValue(undefined);
		mockState.handleApiError.mockReset();
		mockState.listFolder.mockReset();
		mockState.listRoot.mockReset();
		mockState.teams = [];
		mockState.teamsLoading = false;
		mockState.toastSuccess.mockReset();
		mockState.createFolder.mockResolvedValue(undefined);
		mockState.listRoot.mockResolvedValue({ folders: [] } as never);
		mockState.listFolder.mockResolvedValue({ folders: [] } as never);
	});

	it("shows loading, navigates into folders, and confirms the selected target", async () => {
		let resolveRoot:
			| ((value: { folders: (typeof rootFolder)[] }) => void)
			| undefined;

		mockState.listRoot.mockImplementationOnce(
			() =>
				new Promise<{ folders: (typeof rootFolder)[] }>((resolve) => {
					resolveRoot = resolve;
				}),
		);
		mockState.listFolder.mockResolvedValueOnce({
			folders: [childFolder],
		} as never);

		const { onConfirm, onOpenChange } = renderDialog();

		expect(screen.getByText("batch-move")).toBeInTheDocument();
		expect(screen.getByText("target-desc")).toBeInTheDocument();
		expect(screen.getByText("skeleton:6")).toBeInTheDocument();

		resolveRoot?.({ folders: [rootFolder] });

		const rootButton = await screen.findByRole("button", {
			name: "Projects",
		});
		fireEvent.click(rootButton);

		await waitFor(() => {
			expect(mockState.listFolder).toHaveBeenCalledWith(
				{ kind: "personal" },
				1,
				{
					file_limit: 0,
					folder_limit: FOLDER_LIMIT,
				},
			);
		});
		expect(screen.getByText("current:Projects")).toBeInTheDocument();

		const confirmButton = screen.getByRole("button", { name: "move-here" });
		await waitFor(() => {
			expect(confirmButton).toBeEnabled();
		});
		fireEvent.click(confirmButton);

		await waitFor(() => {
			expect(onConfirm).toHaveBeenCalledWith({
				workspace: { kind: "personal" },
				folderId: 1,
			});
		});
		expect(onOpenChange).toHaveBeenCalledWith(false);
	});

	it("lets copy targets switch workspace and resets the browser to that root", async () => {
		mockState.teams = [{ id: 7, name: "Design Team" }];
		mockState.listRoot
			.mockResolvedValueOnce({ folders: [rootFolder] } as never)
			.mockResolvedValueOnce({ folders: [draftsFolder] } as never);

		const { onConfirm } = renderDialog({
			mode: "copy",
			currentFolderId: null,
		});

		await screen.findByRole("button", { name: "Projects" });
		expect(mockState.ensureTeamsLoaded).toHaveBeenCalledWith(42);
		fireEvent.change(screen.getByLabelText("target-workspace"), {
			target: { value: "team:7" },
		});

		await waitFor(() => {
			expect(mockState.listRoot).toHaveBeenLastCalledWith(
				{ kind: "team", teamId: 7 },
				{
					file_limit: 0,
					folder_limit: FOLDER_LIMIT,
				},
			);
		});
		expect(screen.getByText("workspace:Design Team")).toBeInTheDocument();
		expect(screen.getByText("current:Root")).toBeInTheDocument();
		expect(screen.getByRole("button", { name: "Drafts" })).toBeInTheDocument();

		fireEvent.click(screen.getByRole("button", { name: "copy-here" }));

		await waitFor(() => {
			expect(onConfirm).toHaveBeenCalledWith({
				workspace: { kind: "team", teamId: 7 },
				folderId: null,
			});
		});
	});

	it("shows the workspace picker for move targets", async () => {
		mockState.teams = [{ id: 7, name: "Design Team" }];
		renderDialog({ mode: "move" });

		await screen.findByText("empty");
		expect(screen.getByLabelText("target-workspace")).toBeInTheDocument();
		expect(mockState.ensureTeamsLoaded).toHaveBeenCalledWith(42);
	});

	it("blocks descendant targets and lets the user navigate back to a valid parent", async () => {
		mockState.listRoot
			.mockResolvedValueOnce({ folders: [rootFolder] } as never)
			.mockResolvedValueOnce({ folders: [rootFolder] } as never);
		mockState.listFolder.mockResolvedValueOnce({ folders: [] } as never);

		const { onConfirm, onOpenChange } = renderDialog({
			selectedFolderIds: [1],
		});

		fireEvent.click(
			await screen.findByRole("button", {
				name: "Projects",
			}),
		);

		expect(await screen.findByText("invalid-descendant")).toBeInTheDocument();
		expect(screen.getByText("empty")).toBeInTheDocument();

		const confirmButton = screen.getByRole("button", { name: "move-here" });
		expect(confirmButton).toBeDisabled();
		fireEvent.click(confirmButton);
		expect(onConfirm).not.toHaveBeenCalled();

		fireEvent.click(screen.getByRole("button", { name: "back" }));

		await waitFor(() => {
			expect(mockState.listRoot).toHaveBeenCalledTimes(2);
		});
		expect(screen.getByText("current:Root")).toBeInTheDocument();
		expect(screen.queryByText("invalid-descendant")).not.toBeInTheDocument();

		fireEvent.click(screen.getByRole("button", { name: "move-here" }));

		await waitFor(() => {
			expect(onConfirm).toHaveBeenCalledWith({
				workspace: { kind: "personal" },
				folderId: null,
			});
		});
		expect(onOpenChange).toHaveBeenCalledWith(false);
	});

	it("creates a folder in the active target, reloads contents, and hides the create form", async () => {
		mockState.listFolder
			.mockResolvedValueOnce({ folders: [] } as never)
			.mockResolvedValueOnce({ folders: [draftsFolder] } as never);

		renderDialog({
			mode: "copy",
			currentFolderId: 5,
			initialBreadcrumb: [
				{ id: null, name: "Root" },
				{ id: 5, name: "Team" },
			],
		});

		await screen.findByText("current:Team");
		fireEvent.click(screen.getByRole("button", { name: "create-folder" }));

		const input = screen.getByPlaceholderText("folder-name");
		fireEvent.change(input, {
			target: { value: "  Drafts  " },
		});
		fireEvent.keyDown(input, { key: "Enter" });

		await waitFor(() => {
			expect(mockState.createFolder).toHaveBeenCalledWith(
				{ kind: "personal" },
				"Drafts",
				5,
			);
		});
		expect(mockState.toastSuccess).toHaveBeenCalledWith("folder-created");

		await waitFor(() => {
			expect(mockState.listFolder).toHaveBeenLastCalledWith(
				{ kind: "personal" },
				5,
				{
					file_limit: 0,
					folder_limit: FOLDER_LIMIT,
				},
			);
		});
		expect(
			screen.queryByPlaceholderText("folder-name"),
		).not.toBeInTheDocument();
		expect(screen.getByRole("button", { name: "Drafts" })).toBeInTheDocument();
		expect(
			screen.getByRole("button", { name: "copy-here" }),
		).toBeInTheDocument();
	});

	it("does not submit the inline folder form while IME composition is active", async () => {
		renderDialog();

		await screen.findByText("empty");
		fireEvent.click(screen.getByRole("button", { name: "create-folder" }));

		const input = screen.getByPlaceholderText("folder-name");
		fireEvent.change(input, {
			target: { value: "bao" },
		});
		fireEvent.compositionStart(input);
		fireEvent.keyDown(input, { key: "Enter" });

		expect(mockState.createFolder).not.toHaveBeenCalled();
		expect(screen.getByPlaceholderText("folder-name")).toBeInTheDocument();
	});

	it("reports load failures and falls back to the empty state", async () => {
		const error = new Error("load failed");
		mockState.listRoot.mockRejectedValueOnce(error);

		renderDialog();

		await waitFor(() => {
			expect(mockState.handleApiError).toHaveBeenCalledWith(error);
		});
		expect(screen.getByText("empty")).toBeInTheDocument();
		expect(screen.getByText("empty-desc")).toBeInTheDocument();
	});

	it("keeps the create form open when folder creation fails", async () => {
		const error = new Error("create failed");
		mockState.createFolder.mockRejectedValueOnce(error);

		renderDialog();

		await screen.findByText("empty");
		fireEvent.click(screen.getByRole("button", { name: "create-folder" }));

		const input = screen.getByPlaceholderText("folder-name");
		fireEvent.change(input, {
			target: { value: "Archive" },
		});
		fireEvent.click(
			screen.getAllByRole("button", { name: "create-folder" })[1],
		);

		await waitFor(() => {
			expect(mockState.handleApiError).toHaveBeenCalledWith(error);
		});
		expect(input).toHaveValue("Archive");
		expect(screen.getByPlaceholderText("folder-name")).toBeInTheDocument();
	});
});
