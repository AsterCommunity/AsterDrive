import {
	act,
	fireEvent,
	render,
	screen,
	waitFor,
	within,
} from "@testing-library/react";
import { type Ref, useEffect } from "react";
import { beforeEach, describe, expect, it, vi } from "vitest";
import { FILE_BROWSER_FEEDBACK_DURATION_MS } from "@/lib/constants";
import {
	clearStorageEventEchoes,
	consumeStorageEventEcho,
} from "@/lib/storageEventEcho";
import FileBrowserPage from "@/pages/FileBrowserPage";

const mockState = vi.hoisted(() => ({
	batchDelete: vi.fn(),
	createArchiveCompressTask: vi.fn(),
	createArchiveExtractTask: vi.fn(),
	copyFile: vi.fn(),
	copyFolder: vi.fn(),
	getArchivePreview: vi.fn(),
	createPreviewLink: vi.fn(),
	createWopiSession: vi.fn(),
	streamArchiveDownload: vi.fn(),
	startAuthenticatedDownload: vi.fn(),
	dispatchEvent: vi.fn(),
	fileBrowserContext: null as Record<string, unknown> | null,
	folderPolicyPreload: vi.fn(),
	formatBatchToast: vi.fn(),
	handleApiError: vi.fn(),
	idleTasks: [] as Array<() => void>,
	musicPlayTracks: vi.fn(),
	location: {
		pathname: "/folder/12",
		search: "?name=Projects",
		state: null as Record<string, unknown> | null,
	},
	navigate: vi.fn(),
	params: { folderId: "12" as string | undefined },
	previewAppStore: {
		isLoaded: false,
		load: vi.fn(async () => {}),
	},
	thumbnailSupportStore: {
		config: {
			audio_thumbnail: { enabled: false, extensions: [] },
			extensions: ["jpg", "jpeg", "png", "webp"],
			image_preview: {
				enabled: true,
				extensions: ["jpg", "jpeg", "png", "webp"],
			},
			image_thumbnail: {
				enabled: true,
				extensions: ["jpg", "jpeg", "png", "webp"],
			},
			version: 1,
			video_thumbnail: { enabled: false, extensions: [] },
		},
		isLoaded: true,
		load: vi.fn(async () => {}),
	},
	readInternalDragData: vi.fn(),
	refreshUser: vi.fn(),
	resolveResourceHandle: vi.fn(
		async (
			fileId: number,
			request: { delivery_mode: string; representation?: string },
		) => ({
			kind: "ready",
			identity: {
				cacheKey: `/files/${fileId}/download`,
				etag: null,
				scope: "personal",
			},
			request: {
				url: `/files/${fileId}/download?disposition=inline`,
				credentials: "include",
				conditionalHeaders: "allowed",
				redirectPolicy: "same_origin_only",
			},
			delivery: {
				mode: request.delivery_mode,
			},
		}),
	),
	authUser: {
		id: 1,
		role: "user" as "admin" | "user",
	},
	searchParams: new URLSearchParams("name=Projects"),
	setFileLock: vi.fn(),
	setFolderLock: vi.fn(),
	store: {
		breadcrumb: [
			{ id: null, name: "Root" },
			{ id: 12, name: "Projects" },
		] as Array<{ id: number | null; name: string }>,
		clearSelection: vi.fn(),
		currentFolderId: 12 as number | null,
		deleteFile: vi.fn(),
		deleteFolder: vi.fn(),
		error: null as string | null,
		files: [] as Array<Record<string, unknown>>,
		folders: [] as Array<Record<string, unknown>>,
		hasMoreFiles: vi.fn(),
		loadMoreFiles: vi.fn(),
		loading: false,
		loadingMore: false,
		moveToFolder: vi.fn(),
		navigateTo: vi.fn(),
		refresh: vi.fn(),
		browserOpenMode: "single_click" as "single_click" | "double_click",
		setSortBy: vi.fn(),
		setSortOrder: vi.fn(),
		setViewMode: vi.fn(),
		sortBy: "name",
		sortOrder: "asc",
		viewMode: "grid" as "grid" | "list",
	},
	toastError: vi.fn(),
	toastSuccess: vi.fn(),
	triggerFileUpload: vi.fn(),
	triggerFolderUpload: vi.fn(),
	uploadAreaConnected: true,
	useKeyboardShortcuts: vi.fn(),
	warmupPreviewEngines: vi.fn(),
	workspace: {
		kind: "personal" as const,
	} as { kind: "personal" } | { kind: "team"; teamId: number },
}));

class MockIntersectionObserver {
	static instances: MockIntersectionObserver[] = [];

	disconnect = vi.fn();
	observe = vi.fn();
	root = null;
	rootMargin = "";
	thresholds: number[] = [];
	unobserve = vi.fn();

	private readonly callback: IntersectionObserverCallback;

	constructor(
		callback: IntersectionObserverCallback,
		options: IntersectionObserverInit = {},
	) {
		this.callback = callback;
		this.root = (options.root as Element | Document | null | undefined) ?? null;
		this.rootMargin = options.rootMargin ?? "";
		this.thresholds = Array.isArray(options.threshold)
			? options.threshold
			: options.threshold !== undefined
				? [options.threshold]
				: [];
		MockIntersectionObserver.instances.push(this);
	}

	takeRecords() {
		return [];
	}

	trigger(target: Element, isIntersecting = true) {
		this.callback(
			[
				{
					boundingClientRect: DOMRect.fromRect(),
					intersectionRatio: isIntersecting ? 1 : 0,
					intersectionRect: DOMRect.fromRect(),
					isIntersecting,
					rootBounds: null,
					target,
					time: 0,
				} as IntersectionObserverEntry,
			],
			this as unknown as IntersectionObserver,
		);
	}

	static reset() {
		MockIntersectionObserver.instances = [];
	}
}

vi.mock("@/lib/idleTask", () => ({
	runWhenIdle: (task: () => void) => {
		mockState.idleTasks.push(task);
		return () => undefined;
	},
}));

vi.mock("@/lib/pwaWarmup", () => ({
	warmupPreviewEngines: () => mockState.warmupPreviewEngines(),
}));

vi.mock("react-i18next", () => ({
	useTranslation: () => ({
		t: (key: string) => key,
	}),
}));

vi.mock("react-router-dom", () => ({
	useLocation: () => mockState.location,
	useNavigate: () => mockState.navigate,
	useParams: () => mockState.params,
	useSearchParams: () => [mockState.searchParams, vi.fn()],
}));

vi.mock("sonner", () => ({
	toast: {
		error: (...args: unknown[]) => mockState.toastError(...args),
		success: (...args: unknown[]) => mockState.toastSuccess(...args),
	},
}));

vi.mock("@/stores/previewAppStore", () => ({
	usePreviewAppStore: (
		selector: (state: typeof mockState.previewAppStore) => unknown,
	) => selector(mockState.previewAppStore),
}));

vi.mock("@/stores/thumbnailSupportStore", () => ({
	useThumbnailSupportStore: (
		selector: (state: typeof mockState.thumbnailSupportStore) => unknown,
	) => selector(mockState.thumbnailSupportStore),
}));

vi.mock("@/pages/file-browser/useFileBrowserBatchActions", () => ({
	useFileBrowserBatchActions: ({
		onArchiveCompress,
		onArchiveDownload,
	}: {
		onArchiveCompress: (
			fileIds: number[],
			folderIds: number[],
		) => Promise<void> | void;
		onArchiveDownload: (
			fileIds: number[],
			folderIds: number[],
		) => Promise<void>;
	}) => ({
		selectionToolbar: null,
		dialogs: (
			<div>
				<div>batch-action-dialogs</div>
				<button type="button" onClick={() => void onArchiveCompress([3], [])}>
					batch-archive-compress
				</button>
				<button type="button" onClick={() => void onArchiveDownload([], [])}>
					batch-archive-empty
				</button>
			</div>
		),
	}),
}));

vi.mock("@/components/common/EmptyState", () => ({
	EmptyState: ({
		description,
		title,
	}: {
		description?: string;
		title: string;
	}) => (
		<div>
			<div>{title}</div>
			<div>{description}</div>
		</div>
	),
}));

vi.mock("@/components/common/SkeletonFileGrid", () => ({
	SkeletonFileGrid: () => <div>skeleton-grid</div>,
}));

vi.mock("@/components/common/SkeletonFileTable", () => ({
	SkeletonFileTable: () => <div>skeleton-table</div>,
}));

vi.mock("@/components/common/SortMenu", () => ({
	SortMenu: ({
		onSortBy,
		onSortOrder,
		sortBy,
		sortOrder,
	}: {
		onSortBy: (value: string) => void;
		onSortOrder: (value: string) => void;
		sortBy: string;
		sortOrder: string;
	}) => (
		<div>
			<div>{`sort:${sortBy}:${sortOrder}`}</div>
			<button type="button" onClick={() => onSortBy("updated_at")}>
				sort-by-updated
			</button>
			<button type="button" onClick={() => onSortOrder("desc")}>
				sort-order-desc
			</button>
		</div>
	),
}));

vi.mock("@/components/common/ToolbarBar", () => ({
	ToolbarBar: ({
		left,
		right,
	}: {
		left?: React.ReactNode;
		right?: React.ReactNode;
	}) => (
		<div>
			<div>{left}</div>
			<div>{right}</div>
		</div>
	),
}));

vi.mock("@/components/common/ViewToggle", () => ({
	ViewToggle: ({
		onChange,
		value,
	}: {
		onChange: (value: "grid" | "list") => void;
		value: "grid" | "list";
	}) => (
		<div>
			<div>{`view:${value}`}</div>
			<button type="button" onClick={() => onChange("grid")}>
				view-grid
			</button>
			<button type="button" onClick={() => onChange("list")}>
				view-list
			</button>
		</div>
	),
}));

vi.mock("@/components/files/FileBrowserContext", () => ({
	FileBrowserProvider: ({
		children,
		value,
	}: {
		children: React.ReactNode;
		value: Record<string, unknown>;
	}) => {
		mockState.fileBrowserContext = value;
		return children;
	},
}));

vi.mock("@/components/files/FileGrid", () => ({
	FileGrid: () => {
		const context = mockState.fileBrowserContext as {
			files: Array<
				Record<string, unknown> & {
					id: number;
					mime_type?: string;
					name: string;
				}
			>;
			folders: Array<{ id: number; name: string }>;
			onArchiveDownload: (folderId: number) => void;
			onCopy: (type: "file" | "folder", id: number) => void;
			onFileChooseOpenMethod: (file: {
				id: number;
				mime_type: string;
				name: string;
			}) => void;
			onFileClick: (file: {
				id: number;
				mime_type: string;
				name: string;
			}) => void;
			onFileOpen: (file: {
				id: number;
				mime_type: string;
				name: string;
			}) => void;
			onFolderOpen: (id: number, name: string) => void;
			onMoveToFolder: (
				fileIds: number[],
				folderIds: number[],
				targetFolderId: number | null,
			) => Promise<void>;
			onShare: (target: {
				fileId?: number;
				folderId?: number;
				name: string;
				initialMode?: "page" | "direct";
			}) => void;
		} | null;
		const folders = context?.folders ?? [];
		const files = context?.files ?? [];
		const defaultClickFile = {
			id: 3,
			mime_type: "application/pdf",
			name: "report.pdf",
		};
		const clickFile =
			files.find(
				(file) =>
					file.file_category === "audio" ||
					file.mime_type?.startsWith("audio/"),
			) ?? defaultClickFile;

		return (
			<div>
				<div>{`grid:${folders.length}:${files.length}`}</div>
				<button
					type="button"
					onClick={() => context?.onFolderOpen(5, "Docs A")}
				>
					open-folder
				</button>
				<button type="button" onClick={() => context?.onFileClick(clickFile)}>
					open-file
				</button>
				<button type="button" onClick={() => context?.onFileOpen(clickFile)}>
					open-file-direct
				</button>
				<button
					type="button"
					onClick={() => context?.onFileChooseOpenMethod(clickFile)}
				>
					open-file-picker
				</button>
				<button type="button" onClick={() => context?.onCopy("file", 9)}>
					copy-file
				</button>
				<button type="button" onClick={() => context?.onCopy("folder", 10)}>
					copy-folder
				</button>
				<button
					type="button"
					onClick={() =>
						context?.onShare({
							folderId: 5,
							name: "Docs A",
							initialMode: "page",
						})
					}
				>
					share-folder
				</button>
				<button
					type="button"
					onClick={() =>
						context?.onShare({
							fileId: 3,
							name: "report.pdf",
							initialMode: "page",
						})
					}
				>
					share-file-page
				</button>
				<button
					type="button"
					onClick={() =>
						context?.onShare({
							fileId: 3,
							name: "report.pdf",
							initialMode: "direct",
						})
					}
				>
					share-file-direct
				</button>
				<button
					type="button"
					onClick={() => void context?.onMoveToFolder([7], [8], 20)}
				>
					move-selection
				</button>
				<button type="button" onClick={() => context?.onArchiveDownload(5)}>
					archive-folder
				</button>
			</div>
		);
	},
}));

vi.mock("@/components/files/FileTable", () => ({
	FileTable: () => <div>table-view</div>,
}));

vi.mock("@/components/files/BatchTargetFolderDialog", () => ({
	BatchTargetFolderDialog: ({
		onOpenChange,
		mode,
		onConfirm,
		open,
	}: {
		onOpenChange?: (open: boolean) => void;
		mode: string;
		onConfirm: (targetFolderId: number | null) => Promise<void>;
		open: boolean;
	}) =>
		open ? (
			<div>
				<div>{`batch-dialog:${mode}`}</div>
				<button type="button" onClick={() => void onConfirm(20)}>
					confirm-batch-dialog
				</button>
				<button type="button" onClick={() => onOpenChange?.(false)}>
					{`close-batch-dialog:${mode}`}
				</button>
			</div>
		) : null,
}));

vi.mock("@/components/files/ArchiveTaskNameDialog", () => ({
	ArchiveTaskNameDialog: ({
		initialName,
		mode,
		onOpenChange,
		onSubmit,
		open,
	}: {
		initialName: string;
		mode: "compress" | "extract";
		onOpenChange?: (open: boolean) => void;
		onSubmit: (name: string | undefined) => Promise<void>;
		open: boolean;
	}) =>
		open ? (
			<div>
				<div>{`archive-dialog:${mode}:${initialName}`}</div>
				<button type="button" onClick={() => void onSubmit(initialName)}>
					confirm-archive-dialog
				</button>
				<button
					type="button"
					onClick={() =>
						void onSubmit(
							mode === "compress" ? "custom-bundle.zip" : "custom-output",
						)
					}
				>
					confirm-archive-dialog-custom
				</button>
				<button type="button" onClick={() => onOpenChange?.(false)}>
					close-archive-dialog
				</button>
			</div>
		) : null,
}));

vi.mock("@/components/files/CreateFileDialog", () => ({
	CreateFileDialog: ({ open }: { open: boolean }) =>
		open ? <div>create-file-dialog</div> : null,
}));

vi.mock("@/components/files/CreateFolderDialog", () => ({
	CreateFolderDialog: ({ open }: { open: boolean }) =>
		open ? <div>create-folder-dialog</div> : null,
}));

vi.mock("@/components/files/FileInfoDialog", () => ({
	FileInfoDialog: ({
		file,
		folder,
		open,
	}: {
		file?: { name: string };
		folder?: { name: string };
		open: boolean;
	}) => (open ? <div>{`info:${file?.name ?? folder?.name ?? ""}`}</div> : null),
}));

vi.mock("@/components/files/FolderPolicyDialog", () => ({
	FolderPolicyDialog: ({
		folder,
		onOpenChange,
		open,
	}: {
		folder?: { id: number; name: string } | null;
		onOpenChange?: (open: boolean) => void;
		open: boolean;
	}) =>
		open ? (
			<div>
				<div>{`folder-policy:${folder?.name ?? ""}`}</div>
				<button type="button" onClick={() => onOpenChange?.(false)}>
					close-folder-policy
				</button>
			</div>
		) : null,
}));

vi.mock("@/components/files/OfflineDownloadDialog", () => ({
	OfflineDownloadDialog: ({
		open,
		targetFolderName,
	}: {
		open: boolean;
		targetFolderName?: string | null;
	}) =>
		open ? <div>{`offline-download:${targetFolderName ?? ""}`}</div> : null,
}));

vi.mock("@/components/files/FilePreview", () => ({
	FilePreview: ({
		file,
		open = true,
		openMode,
		imageNavigation,
		onClose,
		onFileUpdated,
		resources,
	}: {
		file: { id: number; name: string };
		open?: boolean;
		openMode?: string;
		imageNavigation?: {
			nextFile?: { id: number; name: string };
			onNavigate: (file: { id: number; name: string }) => void;
			previousFile?: { id: number; name: string };
		};
		onClose: () => void;
		onFileUpdated?: () => void;
		resources?: {
			actions?: {
				createExternalPreviewLink?: () => unknown;
				loadArchiveManifest?: () => unknown;
				launchWopiSession?: (appKey: string) => unknown;
			};
			resolve?: (fileId: number, request: { delivery_mode: string }) => unknown;
		};
	}) =>
		open ? (
			<div>
				<div
					data-testid="file-preview"
					data-name={file.name}
					data-next-image={imageNavigation?.nextFile?.name ?? ""}
					data-previous-image={imageNavigation?.previousFile?.name ?? ""}
				>
					{`preview:${file.name}:${openMode ?? "auto"}`}
				</div>
				<button type="button" onClick={onClose}>
					close-preview
				</button>
				<button type="button" onClick={onFileUpdated}>
					refresh-preview-file
				</button>
				<button
					type="button"
					onClick={() =>
						resources?.resolve?.(file.id, {
							delivery_mode: "blob_url",
						})
					}
				>
					resolve-preview-resource
				</button>
				<button
					type="button"
					onClick={() => resources?.actions?.createExternalPreviewLink?.()}
				>
					create-preview-link
				</button>
				<button
					type="button"
					onClick={() => resources?.actions?.loadArchiveManifest?.()}
				>
					get-archive-preview
				</button>
				<button
					type="button"
					onClick={() => resources?.actions?.launchWopiSession?.("office")}
				>
					create-wopi-session
				</button>
				<button
					type="button"
					disabled={!imageNavigation?.previousFile}
					onClick={() => {
						if (imageNavigation?.previousFile) {
							imageNavigation.onNavigate(imageNavigation.previousFile);
						}
					}}
				>
					previous-image
				</button>
				<button
					type="button"
					disabled={!imageNavigation?.nextFile}
					onClick={() => {
						if (imageNavigation?.nextFile) {
							imageNavigation.onNavigate(imageNavigation.nextFile);
						}
					}}
				>
					next-image
				</button>
			</div>
		) : null,
}));

vi.mock("@/components/files/RenameDialog", () => ({
	RenameDialog: ({
		currentName,
		open,
	}: {
		currentName: string;
		open: boolean;
	}) => (open ? <div>{`rename:${currentName}`}</div> : null),
}));

vi.mock("@/components/files/ShareDialog", () => ({
	ShareDialog: ({
		name,
		onOpenChange,
		onShareCreated,
		open,
		initialMode,
	}: {
		name: string;
		onOpenChange?: (open: boolean) => void;
		onShareCreated?: () => void | Promise<void>;
		open: boolean;
		initialMode?: "page" | "direct";
	}) =>
		open ? (
			<div>
				<div>{`share:${name}:${initialMode ?? "page"}`}</div>
				<button
					type="button"
					onClick={() =>
						void Promise.resolve(onShareCreated?.()).catch(() => undefined)
					}
				>
					create-share-success
				</button>
				<button
					type="button"
					onClick={() => {
						mockState.store.refresh.mockRejectedValueOnce(
							new Error("refresh failed"),
						);
						void Promise.resolve(onShareCreated?.()).catch(() => undefined);
					}}
				>
					create-share-refresh-fails
				</button>
				<button type="button" onClick={() => onOpenChange?.(false)}>
					close-share-dialog
				</button>
			</div>
		) : null,
}));

vi.mock("@/components/files/UploadArea", () => ({
	UploadArea: function MockUploadArea({
		children,
		ref,
	}: {
		children: React.ReactNode;
		ref?: Ref<{
			triggerFileUpload: () => void;
			triggerFolderUpload: () => void;
		}>;
	}) {
		const connected = mockState.uploadAreaConnected;
		// biome-ignore lint/correctness/useExhaustiveDependencies: test mock reads an external flag so rerender can simulate ref disconnect/reconnect.
		useEffect(() => {
			if (typeof ref !== "function") return;
			ref(
				connected
					? {
							triggerFileUpload: mockState.triggerFileUpload,
							triggerFolderUpload: mockState.triggerFolderUpload,
						}
					: null,
			);
			return () => ref(null);
		}, [connected, ref]);
		return <div>{children}</div>;
	},
}));

vi.mock("@/components/files/VersionHistoryDialog", () => ({
	VersionHistoryDialog: ({
		onOpenChange,
		onRestored,
		open,
	}: {
		onOpenChange?: (open: boolean) => void;
		onRestored?: () => void;
		open: boolean;
	}) =>
		open ? (
			<div>
				<div>version-history-dialog</div>
				<button type="button" onClick={() => onOpenChange?.(false)}>
					close-version-history
				</button>
				<button type="button" onClick={onRestored}>
					restore-version-history
				</button>
			</div>
		) : null,
}));

vi.mock("@/components/layout/AppLayout", () => ({
	AppLayout: ({
		children,
		onMoveToFolder,
		onTrashDrop,
	}: {
		children: React.ReactNode;
		onMoveToFolder?: (
			fileIds: number[],
			folderIds: number[],
			targetFolderId: number | null,
		) => Promise<void>;
		onTrashDrop?: (data: {
			fileIds: number[];
			folderIds: number[];
		}) => Promise<void>;
	}) => (
		<div>
			<button type="button" onClick={() => void onMoveToFolder?.([1], [2], 30)}>
				layout-move
			</button>
			<button
				type="button"
				onClick={() => void onTrashDrop?.({ fileIds: [1], folderIds: [2] })}
			>
				layout-trash
			</button>
			<div>{children}</div>
		</div>
	),
}));

vi.mock("@/components/ui/breadcrumb", () => ({
	Breadcrumb: ({ children }: { children: React.ReactNode }) => (
		<div>{children}</div>
	),
	BreadcrumbEllipsis: () => <span>ellipsis</span>,
	BreadcrumbItem: ({ children }: { children: React.ReactNode }) => (
		<div>{children}</div>
	),
	BreadcrumbLink: ({
		children,
		className,
		onClick,
		onDragOver,
		onDragLeave,
		onDrop,
	}: {
		children: React.ReactNode;
		className?: string;
		onClick?: () => void;
		onDragOver?: (event: React.DragEvent<HTMLButtonElement>) => void;
		onDragLeave?: (event: React.DragEvent<HTMLButtonElement>) => void;
		onDrop?: (event: React.DragEvent<HTMLButtonElement>) => void;
	}) => (
		<button
			type="button"
			className={className}
			onClick={onClick}
			onDragOver={onDragOver}
			onDragLeave={onDragLeave}
			onDrop={onDrop}
		>
			{children}
		</button>
	),
	BreadcrumbList: ({ children }: { children: React.ReactNode }) => (
		<div>{children}</div>
	),
	BreadcrumbPage: ({ children }: { children: React.ReactNode }) => (
		<span>{children}</span>
	),
	BreadcrumbSeparator: ({ children }: { children?: React.ReactNode }) => (
		<span>{children ?? "/"}</span>
	),
}));

vi.mock("@/components/ui/dropdown-menu", () => ({
	DropdownMenu: ({ children }: { children: React.ReactNode }) => (
		<div>{children}</div>
	),
	DropdownMenuTrigger: ({ render }: { render: React.ReactNode }) => render,
	DropdownMenuContent: ({
		children,
		className,
	}: {
		children: React.ReactNode;
		className?: string;
	}) => <div className={className}>{children}</div>,
	DropdownMenuItem: ({
		children,
		disabled,
		onClick,
	}: {
		children: React.ReactNode;
		disabled?: boolean;
		onClick?: () => void;
	}) => (
		<button type="button" disabled={disabled} onClick={onClick}>
			{children}
		</button>
	),
	DropdownMenuSeparator: () => <hr />,
}));

vi.mock("@/components/ui/context-menu", () => ({
	ContextMenu: ({ children }: { children: React.ReactNode }) => (
		<div>{children}</div>
	),
	ContextMenuContent: ({ children }: { children: React.ReactNode }) => (
		<div>{children}</div>
	),
	ContextMenuItem: ({
		children,
		disabled,
		onClick,
	}: {
		children: React.ReactNode;
		disabled?: boolean;
		onClick?: () => void;
	}) => (
		<button type="button" disabled={disabled} onClick={onClick}>
			{children}
		</button>
	),
	ContextMenuSeparator: () => <hr data-testid="context-menu-separator" />,
	ContextMenuTrigger: ({
		children,
		className,
	}: {
		children: React.ReactNode;
		className?: string;
	}) => <div className={className}>{children}</div>,
}));

vi.mock("@/components/ui/icon", () => ({
	Icon: ({ name }: { name: string }) => <span>{name}</span>,
}));

vi.mock("@/components/ui/scroll-area", () => ({
	ScrollArea: function MockScrollArea({
		children,
		className,
		ref,
	}: {
		children: React.ReactNode;
		className?: string;
		ref?: Ref<HTMLDivElement>;
	}) {
		return (
			<div ref={ref} className={className}>
				{children}
			</div>
		);
	},
}));

vi.mock("@/hooks/useApiError", () => ({
	handleApiError: (...args: unknown[]) => mockState.handleApiError(...args),
}));

vi.mock("@/hooks/useKeyboardShortcuts", () => ({
	useKeyboardShortcuts: () => mockState.useKeyboardShortcuts(),
}));

vi.mock("@/lib/dragDrop", () => ({
	getInvalidInternalDropReason: vi.fn(() => null),
	hasInternalDragData: vi.fn(() => false),
	readInternalDragData: (...args: unknown[]) =>
		mockState.readInternalDragData(...args),
}));

vi.mock("@/lib/formatBatchToast", () => ({
	formatBatchToast: (...args: unknown[]) => mockState.formatBatchToast(...args),
}));

vi.mock("@/lib/utils", () => ({
	cn: (...values: Array<string | false | null | undefined>) =>
		values.filter(Boolean).join(" "),
}));

vi.mock("@/services/batchService", () => ({
	batchService: {
		batchDelete: (...args: unknown[]) => mockState.batchDelete(...args),
		createArchiveCompressTask: (...args: unknown[]) =>
			mockState.createArchiveCompressTask(...args),
		streamArchiveDownload: (...args: unknown[]) =>
			mockState.streamArchiveDownload(...args),
	},
}));

vi.mock("@/services/fileService", () => ({
	fileService: {
		copyFile: (...args: unknown[]) => mockState.copyFile(...args),
		copyFolder: (...args: unknown[]) => mockState.copyFolder(...args),
		createArchiveExtractTask: (...args: unknown[]) =>
			mockState.createArchiveExtractTask(...args),
		createPreviewLink: (...args: unknown[]) =>
			mockState.createPreviewLink(...args),
		getArchivePreview: (...args: unknown[]) =>
			mockState.getArchivePreview(...args),
		createWopiSession: (...args: unknown[]) =>
			mockState.createWopiSession(...args),
		downloadPath: (id: number) => `/files/${id}/download`,
		getMediaMetadata: vi.fn(async () => null),
		imagePreviewPath: (id: number) => `/files/${id}/image-preview`,
		resolveResourceHandle: (...args: unknown[]) =>
			mockState.resolveResourceHandle(...args),
		setFileLock: (...args: unknown[]) => mockState.setFileLock(...args),
		setFolderLock: (...args: unknown[]) => mockState.setFolderLock(...args),
		thumbnailPath: (id: number) => `/files/${id}/thumbnail`,
	},
}));

vi.mock("@/lib/authenticatedDownload", () => ({
	startAuthenticatedDownload: (...args: unknown[]) =>
		mockState.startAuthenticatedDownload(...args),
}));

vi.mock("@/stores/authStore", () => {
	const useAuthStore = <T,>(
		selector: (state: {
			refreshUser: typeof mockState.refreshUser;
			user: typeof mockState.authUser;
		}) => T,
	) =>
		selector({
			refreshUser: mockState.refreshUser,
			user: mockState.authUser,
		});

	useAuthStore.getState = () => ({
		refreshUser: mockState.refreshUser,
		user: mockState.authUser,
	});

	return { useAuthStore };
});

vi.mock("@/stores/fileStore", () => {
	const useFileStore = <T,>(selector: (state: typeof mockState.store) => T) =>
		selector(mockState.store);

	useFileStore.getState = () => mockState.store;

	return { useFileStore };
});

vi.mock("@/stores/musicPlayerStore", () => ({
	useMusicPlayerStore: (
		selector: (state: {
			playTracks: typeof mockState.musicPlayTracks;
		}) => unknown,
	) =>
		selector({
			playTracks: mockState.musicPlayTracks,
		}),
}));

vi.mock("@/stores/workspaceStore", () => ({
	bindWorkspaceService: <T extends object>(
		factory: (
			workspace: { kind: "personal" } | { kind: "team"; teamId: number },
		) => T,
	) => factory(mockState.workspace),
	useWorkspaceStore: Object.assign(
		<T,>(
			selector: (state: {
				workspace: { kind: "personal" } | { kind: "team"; teamId: number };
			}) => T,
		) => selector({ workspace: mockState.workspace }),
		{
			getState: () => ({ workspace: mockState.workspace }),
		},
	),
}));

function createFolder(overrides: Record<string, unknown> = {}) {
	return {
		created_at: "2026-03-28T00:00:00Z",
		id: 5,
		is_locked: false,
		name: "Docs",
		updated_at: "2026-03-28T00:00:00Z",
		...overrides,
	};
}

function createFile(overrides: Record<string, unknown> = {}) {
	return {
		created_at: "2026-03-28T00:00:00Z",
		id: 3,
		is_locked: false,
		mime_type: "text/plain",
		name: "notes.txt",
		size: 10,
		updated_at: "2026-03-28T00:00:00Z",
		...overrides,
	};
}

function getFileBrowserContext() {
	const context = mockState.fileBrowserContext as {
		onArchiveExtract: (fileId: number) => void;
		onDelete: (type: "file" | "folder", id: number) => Promise<void>;
		onDownload: (fileId: number, fileName: string) => void;
		onFileClick: (file: Record<string, unknown>) => void;
		onFolderPolicy?: (folder: { id: number; name: string }) => void;
		onInfo: (type: "file" | "folder", id: number) => void;
		onMove: (type: "file" | "folder", id: number) => void;
		onRename: (type: "file" | "folder", id: number, name: string) => void;
		onToggleLock: (
			type: "file" | "folder",
			id: number,
			locked: boolean,
		) => Promise<boolean>;
		onVersions: (fileId: number) => void;
	} | null;

	if (!context) {
		throw new Error("missing file browser context");
	}

	return context;
}

describe("FileBrowserPage", () => {
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

		MockIntersectionObserver.reset();
		mockState.batchDelete.mockReset();
		mockState.createArchiveCompressTask.mockReset();
		mockState.createArchiveExtractTask.mockReset();
		mockState.copyFile.mockReset();
		mockState.copyFolder.mockReset();
		mockState.getArchivePreview.mockReset();
		mockState.createPreviewLink.mockReset();
		mockState.createWopiSession.mockReset();
		mockState.streamArchiveDownload.mockReset();
		mockState.startAuthenticatedDownload.mockReset();
		mockState.startAuthenticatedDownload.mockResolvedValue(undefined);
		mockState.dispatchEvent.mockReset();
		mockState.fileBrowserContext = null;
		mockState.folderPolicyPreload.mockReset();
		mockState.formatBatchToast.mockReset();
		mockState.handleApiError.mockReset();
		mockState.idleTasks = [];
		mockState.musicPlayTracks.mockReset();
		mockState.location = {
			pathname: "/folder/12",
			search: "?name=Projects",
			state: null,
		};
		mockState.navigate.mockReset();
		mockState.previewAppStore.load.mockReset();
		mockState.thumbnailSupportStore.config = {
			audio_thumbnail: { enabled: false, extensions: [] },
			extensions: ["jpg", "jpeg", "png", "webp"],
			image_preview: {
				enabled: true,
				extensions: ["jpg", "jpeg", "png", "webp"],
			},
			image_thumbnail: {
				enabled: true,
				extensions: ["jpg", "jpeg", "png", "webp"],
			},
			version: 1,
			video_thumbnail: { enabled: false, extensions: [] },
		};
		mockState.thumbnailSupportStore.isLoaded = true;
		mockState.thumbnailSupportStore.load.mockReset();
		mockState.readInternalDragData.mockReset();
		mockState.refreshUser.mockReset();
		mockState.resolveResourceHandle.mockClear();
		mockState.authUser = {
			id: 1,
			role: "user",
		};
		mockState.setFileLock.mockReset();
		mockState.setFolderLock.mockReset();
		mockState.store.clearSelection.mockReset();
		mockState.store.deleteFile.mockReset();
		mockState.store.deleteFolder.mockReset();
		mockState.store.hasMoreFiles.mockReset();
		mockState.store.loadMoreFiles.mockReset();
		mockState.store.moveToFolder.mockReset();
		mockState.store.navigateTo.mockReset();
		mockState.store.refresh.mockReset();
		mockState.store.setSortBy.mockReset();
		mockState.store.setSortOrder.mockReset();
		mockState.store.setViewMode.mockReset();
		mockState.toastError.mockReset();
		mockState.toastSuccess.mockReset();
		mockState.triggerFileUpload.mockReset();
		mockState.triggerFolderUpload.mockReset();
		mockState.uploadAreaConnected = true;
		mockState.useKeyboardShortcuts.mockReset();
		mockState.warmupPreviewEngines.mockReset();
		mockState.workspace = { kind: "personal" };

		mockState.params = { folderId: "12" };
		mockState.previewAppStore.isLoaded = false;
		mockState.previewAppStore.load.mockResolvedValue(undefined);
		mockState.searchParams = new URLSearchParams("name=Projects");
		mockState.store.browserOpenMode = "single_click";
		mockState.store.breadcrumb = [
			{ id: null, name: "Root" },
			{ id: 12, name: "Projects" },
		];
		mockState.store.currentFolderId = 12;
		mockState.store.error = null;
		mockState.store.files = [createFile()];
		mockState.store.folders = [createFolder()];
		mockState.store.hasMoreFiles.mockReturnValue(false);
		mockState.store.loading = false;
		mockState.store.loadingMore = false;
		mockState.store.moveToFolder.mockResolvedValue({ ok: true });
		mockState.store.navigateTo.mockResolvedValue(undefined);
		mockState.store.refresh.mockResolvedValue(undefined);
		mockState.store.sortBy = "name";
		mockState.store.sortOrder = "asc";
		mockState.store.viewMode = "grid";
		clearStorageEventEchoes();

		mockState.batchDelete.mockResolvedValue({ ok: true });
		mockState.createArchiveCompressTask.mockResolvedValue({
			display_name: "Compress custom-bundle.zip",
		});
		mockState.createArchiveExtractTask.mockResolvedValue({
			display_name: "Extract bundle.zip",
		});
		mockState.copyFile.mockResolvedValue(undefined);
		mockState.copyFolder.mockResolvedValue(undefined);
		mockState.getArchivePreview.mockResolvedValue({ entries: [] });
		mockState.createPreviewLink.mockResolvedValue("preview-link");
		mockState.createWopiSession.mockResolvedValue({ session: "wopi" });
		mockState.formatBatchToast.mockImplementation((_t, action: string) => ({
			description: `${action}:desc`,
			title: `${action}:ok`,
			variant: "success",
		}));
		mockState.refreshUser.mockResolvedValue(undefined);
		mockState.readInternalDragData.mockReturnValue(null);

		vi.spyOn(document, "dispatchEvent").mockImplementation(
			(...args: [Event]) => {
				mockState.dispatchEvent(...args);
				return true;
			},
		);
	});

	it("schedules preview engine warmup after entering the file browser", async () => {
		render(<FileBrowserPage />);

		expect(mockState.warmupPreviewEngines).not.toHaveBeenCalled();
		expect(mockState.idleTasks.length).toBeGreaterThanOrEqual(2);

		for (const idleTask of mockState.idleTasks) {
			idleTask();
		}
		await vi.waitFor(() => {
			expect(mockState.warmupPreviewEngines).toHaveBeenCalledTimes(1);
		});
	});

	it("navigates on mount, renders folder contents in grid view, and wires sort and view controls", async () => {
		render(<FileBrowserPage />);

		await waitFor(() => {
			expect(mockState.store.navigateTo).toHaveBeenCalledWith(12, "Projects");
		});
		expect(mockState.previewAppStore.load).toHaveBeenCalledTimes(1);
		expect(screen.getByText("grid:1:1")).toBeInTheDocument();
		expect(screen.getByText("view:grid")).toBeInTheDocument();
		expect(screen.getByText("sort:name:asc")).toBeInTheDocument();

		fireEvent.click(screen.getByRole("button", { name: "view-list" }));
		fireEvent.click(screen.getByRole("button", { name: "sort-by-updated" }));
		fireEvent.click(screen.getByRole("button", { name: "sort-order-desc" }));

		expect(mockState.store.setViewMode).toHaveBeenCalledWith("list");
		expect(mockState.store.setSortBy).toHaveBeenCalledWith("updated_at");
		expect(mockState.store.setSortOrder).toHaveBeenCalledWith("desc");
	});

	it("does not expose folder policy management to regular users", () => {
		render(<FileBrowserPage />);

		expect(getFileBrowserContext().onFolderPolicy).toBeUndefined();
	});

	it("opens folder policy management for admins", async () => {
		mockState.authUser = {
			id: 1,
			role: "admin",
		};
		render(<FileBrowserPage />);

		const context = getFileBrowserContext();
		expect(context.onFolderPolicy).toBeTypeOf("function");

		act(() => {
			context.onFolderPolicy?.({ id: 5, name: "Docs" });
		});

		expect(await screen.findByText("folder-policy:Docs")).toBeInTheDocument();
		fireEvent.click(
			screen.getByRole("button", { name: "close-folder-policy" }),
		);
		expect(screen.queryByText("folder-policy:Docs")).not.toBeInTheDocument();
	});

	it("refreshes and navigates from breadcrumb and folder open actions, and opens the preview", async () => {
		render(<FileBrowserPage />);

		await waitFor(() => {
			expect(mockState.store.navigateTo).toHaveBeenCalledWith(12, "Projects");
		});

		fireEvent.click(screen.getByTitle("core:refresh"));
		const contextRefreshButton = screen
			.getAllByText("core:refresh")
			.at(-1)
			?.closest("button");
		expect(contextRefreshButton).toBeTruthy();
		if (!contextRefreshButton) {
			throw new Error("missing context menu refresh button");
		}
		fireEvent.click(contextRefreshButton);
		fireEvent.click(screen.getByRole("button", { name: "Root" }));
		fireEvent.click(screen.getByRole("button", { name: "open-folder" }));
		fireEvent.click(screen.getByRole("button", { name: "open-file" }));

		expect(mockState.store.refresh).toHaveBeenCalledTimes(2);
		expect(mockState.navigate).toHaveBeenCalledWith("/");
		expect(mockState.navigate).toHaveBeenCalledWith("/folder/5?name=Docs%20A");
		expect(
			await screen.findByText("preview:report.pdf:auto"),
		).toBeInTheDocument();
	});

	it("passes adjacent image navigation to the preview and updates the preview file when navigating", async () => {
		const firstImage = createFile({
			file_category: "image",
			id: 10,
			mime_type: "image/png",
			name: "first.png",
		});
		const documentFile = createFile({
			file_category: "document",
			id: 11,
			mime_type: "application/pdf",
			name: "notes.pdf",
		});
		const secondImage = createFile({
			file_category: "image",
			id: 12,
			mime_type: "image/jpeg",
			name: "second.jpg",
		});
		const rawImage = createFile({
			id: 13,
			mime_type: "application/octet-stream",
			name: "capture.nef",
		});
		mockState.thumbnailSupportStore.config = {
			audio_thumbnail: { enabled: false, extensions: [] },
			extensions: ["jpg", "jpeg", "nef", "png", "webp"],
			image_preview: {
				enabled: true,
				extensions: ["jpg", "jpeg", "nef", "png", "webp"],
			},
			image_thumbnail: {
				enabled: true,
				extensions: ["jpg", "jpeg", "nef", "png", "webp"],
			},
			version: 1,
			video_thumbnail: { enabled: false, extensions: [] },
		};
		mockState.store.files = [firstImage, documentFile, secondImage, rawImage];

		render(<FileBrowserPage />);

		await waitFor(() => {
			expect(mockState.store.navigateTo).toHaveBeenCalledWith(12, "Projects");
		});

		getFileBrowserContext().onFileClick(firstImage);

		expect(await screen.findByTestId("file-preview")).toHaveAttribute(
			"data-name",
			"first.png",
		);
		expect(screen.getByTestId("file-preview")).toHaveAttribute(
			"data-previous-image",
			"capture.nef",
		);
		expect(screen.getByTestId("file-preview")).toHaveAttribute(
			"data-next-image",
			"second.jpg",
		);

		fireEvent.click(screen.getByRole("button", { name: "next-image" }));
		expect(await screen.findByTestId("file-preview")).toHaveAttribute(
			"data-name",
			"second.jpg",
		);
		expect(screen.getByTestId("file-preview")).toHaveAttribute(
			"data-previous-image",
			"first.png",
		);
		expect(screen.getByTestId("file-preview")).toHaveAttribute(
			"data-next-image",
			"capture.nef",
		);

		fireEvent.click(screen.getByRole("button", { name: "next-image" }));
		expect(await screen.findByTestId("file-preview")).toHaveAttribute(
			"data-name",
			"capture.nef",
		);
		expect(screen.getByTestId("file-preview")).toHaveAttribute(
			"data-previous-image",
			"second.jpg",
		);
		expect(screen.getByTestId("file-preview")).toHaveAttribute(
			"data-next-image",
			"first.png",
		);

		fireEvent.click(screen.getByRole("button", { name: "previous-image" }));
		expect(await screen.findByTestId("file-preview")).toHaveAttribute(
			"data-name",
			"second.jpg",
		);
	});

	it("does not expose image navigation when the current preview file is not in an image queue", async () => {
		const documentFile = createFile({
			file_category: "document",
			id: 21,
			mime_type: "application/pdf",
			name: "manual.pdf",
		});
		mockState.store.files = [
			documentFile,
			createFile({
				file_category: "image",
				id: 22,
				mime_type: "image/png",
				name: "lonely.png",
			}),
		];

		render(<FileBrowserPage />);

		await waitFor(() => {
			expect(mockState.store.navigateTo).toHaveBeenCalledWith(12, "Projects");
		});

		getFileBrowserContext().onFileClick(documentFile);

		expect(await screen.findByTestId("file-preview")).toHaveAttribute(
			"data-name",
			"manual.pdf",
		);
		expect(screen.getByTestId("file-preview")).toHaveAttribute(
			"data-previous-image",
			"",
		);
		expect(screen.getByTestId("file-preview")).toHaveAttribute(
			"data-next-image",
			"",
		);
		expect(
			screen.getByRole("button", { name: "previous-image" }),
		).toBeDisabled();
		expect(screen.getByRole("button", { name: "next-image" })).toBeDisabled();
	});

	it("plays an audio file directly with the current folder music queue", async () => {
		mockState.store.files = [
			createFile({
				file_category: "audio",
				id: 3,
				mime_type: "audio/mpeg",
				name: "Artist - Song.mp3",
				size: 1024,
			}),
			createFile({
				file_category: "audio",
				id: 4,
				mime_type: "audio/flac",
				name: "Second.flac",
				size: 2048,
			}),
			createFile({
				file_category: "document",
				id: 5,
				mime_type: "application/pdf",
				name: "Manual.pdf",
				size: 4096,
			}),
		];

		render(<FileBrowserPage />);

		await waitFor(() => {
			expect(mockState.store.navigateTo).toHaveBeenCalledWith(12, "Projects");
		});

		fireEvent.click(screen.getByRole("button", { name: "open-file" }));

		await waitFor(() => {
			expect(mockState.musicPlayTracks).toHaveBeenCalledWith(
				[
					expect.objectContaining({
						id: "file:3",
						metadata: expect.objectContaining({
							artist: "Artist",
							title: "Song",
						}),
						name: "Artist - Song.mp3",
					}),
					expect.objectContaining({
						id: "file:4",
						name: "Second.flac",
					}),
				],
				"file:3",
			);
		});
		expect(screen.queryByText("preview:Artist - Song.mp3:auto")).toBeNull();
	});

	it("uses a house icon at root and a folder icon in child folders", async () => {
		mockState.params = { folderId: undefined };
		mockState.location = { pathname: "/", search: "", state: null };
		mockState.searchParams = new URLSearchParams();
		mockState.store.breadcrumb = [{ id: null, name: "Root" }];

		const { rerender } = render(<FileBrowserPage />);

		await waitFor(() => {
			expect(mockState.store.navigateTo).toHaveBeenCalledWith(null, undefined);
		});

		const refreshButton = screen.getByTitle("core:refresh");
		const leftSlot = refreshButton.closest("div");
		expect(leftSlot).toBeTruthy();
		if (!leftSlot) {
			throw new Error("missing toolbar left slot");
		}
		expect(within(leftSlot).getByText("House")).toBeInTheDocument();

		mockState.params = { folderId: "12" };
		mockState.location = {
			pathname: "/folder/12",
			search: "?name=Projects",
			state: null,
		};
		mockState.searchParams = new URLSearchParams("name=Projects");
		mockState.store.breadcrumb = [
			{ id: null, name: "Root" },
			{ id: 12, name: "Projects" },
		];

		rerender(<FileBrowserPage />);

		await waitFor(() => {
			expect(mockState.store.navigateTo).toHaveBeenCalledWith(12, "Projects");
		});
		expect(within(leftSlot).getByText("FolderOpen")).toBeInTheDocument();
	});

	it("reloads the current folder when the workspace changes without a folder route change", async () => {
		mockState.params = { folderId: undefined };
		mockState.location = { pathname: "/teams/1", search: "", state: null };
		mockState.searchParams = new URLSearchParams();
		mockState.workspace = { kind: "team", teamId: 1 };
		mockState.store.breadcrumb = [{ id: null, name: "Root" }];
		mockState.store.currentFolderId = null;

		const { rerender } = render(<FileBrowserPage />);

		await waitFor(() => {
			expect(mockState.store.navigateTo).toHaveBeenCalledWith(null, undefined);
		});

		mockState.location = { pathname: "/teams/2", search: "", state: null };
		mockState.workspace = { kind: "team", teamId: 2 };
		rerender(<FileBrowserPage />);

		await waitFor(() => {
			expect(
				mockState.store.navigateTo.mock.calls.filter(
					([nextFolderId, nextFolderName]) =>
						nextFolderId === null && nextFolderName === undefined,
				),
			).toHaveLength(2);
		});
	});

	it("collapses deep breadcrumbs on small screens to root ellipsis current", async () => {
		vi.mocked(window.matchMedia).mockImplementation((query: string) => ({
			matches: query === "(max-width: 639px)",
			media: query,
			onchange: null,
			addEventListener: vi.fn(),
			removeEventListener: vi.fn(),
			addListener: vi.fn(),
			removeListener: vi.fn(),
			dispatchEvent: vi.fn(),
		}));
		mockState.store.breadcrumb = [
			{ id: null, name: "Root" },
			{ id: 1, name: "Workspace" },
			{ id: 2, name: "Semester" },
			{ id: 12, name: "人工智能学院选课名单0320" },
		];

		render(<FileBrowserPage />);

		await waitFor(() => {
			expect(mockState.store.navigateTo).toHaveBeenCalledWith(12, "Projects");
		});

		expect(screen.getByRole("button", { name: "Root" })).toBeInTheDocument();
		expect(
			screen.getByRole("button", { name: "core:more" }),
		).toBeInTheDocument();
		expect(screen.getByText("ellipsis")).toBeInTheDocument();
		expect(
			screen.getByRole("button", { name: /Workspace/ }),
		).toBeInTheDocument();
		expect(
			screen.getByRole("button", { name: /Semester/ }),
		).toBeInTheDocument();
		expect(screen.getByText("人工智能学院选课名单0320")).toBeInTheDocument();

		fireEvent.click(screen.getByRole("button", { name: /Workspace/ }));
		expect(mockState.navigate).toHaveBeenCalledWith("/folder/1?name=Workspace");
	});

	it("groups page context menu actions with separators", () => {
		render(<FileBrowserPage />);

		expect(screen.getAllByTestId("context-menu-separator")).toHaveLength(2);
	});

	it("opens the offline download dialog from the toolbar action", async () => {
		render(<FileBrowserPage />);

		const offlineDownloadButtons = screen.getAllByRole("button", {
			name: /tasks:offline_download_action/,
		});
		fireEvent.click(offlineDownloadButtons[0]);

		expect(
			await screen.findByText("offline-download:Projects"),
		).toBeInTheDocument();
	});

	it("opens create dialogs and triggers uploads from folder action menus", async () => {
		render(<FileBrowserPage />);

		fireEvent.click(screen.getAllByRole("button", { name: /new_file/ })[0]);
		fireEvent.click(screen.getAllByRole("button", { name: /new_folder/ })[0]);

		expect(await screen.findByText("create-file-dialog")).toBeInTheDocument();
		expect(screen.getByText("create-folder-dialog")).toBeInTheDocument();

		fireEvent.click(screen.getAllByRole("button", { name: /upload_file/ })[0]);
		fireEvent.click(
			screen.getAllByRole("button", { name: /upload_folder/ })[0],
		);

		expect(mockState.triggerFileUpload).toHaveBeenCalledTimes(1);
		expect(mockState.triggerFolderUpload).toHaveBeenCalledTimes(1);
	});

	it("disables upload actions while the upload area ref is disconnected and restores them after reconnect", async () => {
		const { rerender } = render(<FileBrowserPage />);

		await waitFor(() => {
			expect(
				screen.getAllByRole("button", { name: /upload_file/ })[0],
			).toBeEnabled();
		});

		mockState.uploadAreaConnected = false;
		rerender(<FileBrowserPage />);

		await waitFor(() => {
			expect(
				screen.getAllByRole("button", { name: /upload_file/ })[0],
			).toBeDisabled();
			expect(
				screen.getAllByRole("button", { name: /upload_folder/ })[0],
			).toBeDisabled();
		});

		fireEvent.click(screen.getAllByRole("button", { name: /upload_file/ })[0]);
		fireEvent.click(
			screen.getAllByRole("button", { name: /upload_folder/ })[0],
		);
		expect(mockState.triggerFileUpload).not.toHaveBeenCalled();
		expect(mockState.triggerFolderUpload).not.toHaveBeenCalled();

		mockState.uploadAreaConnected = true;
		rerender(<FileBrowserPage />);

		await waitFor(() => {
			expect(
				screen.getAllByRole("button", { name: /upload_file/ })[0],
			).toBeEnabled();
			expect(
				screen.getAllByRole("button", { name: /upload_folder/ })[0],
			).toBeEnabled();
		});

		fireEvent.click(screen.getAllByRole("button", { name: /upload_file/ })[0]);
		fireEvent.click(
			screen.getAllByRole("button", { name: /upload_folder/ })[0],
		);
		expect(mockState.triggerFileUpload).toHaveBeenCalledTimes(1);
		expect(mockState.triggerFolderUpload).toHaveBeenCalledTimes(1);
	});

	it("copies files and folders through the batch target dialog and refreshes after success", async () => {
		render(<FileBrowserPage />);

		fireEvent.click(screen.getByRole("button", { name: "copy-file" }));

		expect(await screen.findByText("batch-dialog:copy")).toBeInTheDocument();

		fireEvent.click(
			screen.getByRole("button", { name: "confirm-batch-dialog" }),
		);

		await waitFor(() => {
			expect(mockState.copyFile).toHaveBeenCalledWith(9, 20);
		});
		expect(mockState.toastSuccess).toHaveBeenCalledWith("copy_success");
		expect(mockState.store.refresh).toHaveBeenCalledTimes(1);

		fireEvent.click(screen.getByRole("button", { name: "copy-folder" }));
		fireEvent.click(
			screen.getByRole("button", { name: "confirm-batch-dialog" }),
		);

		await waitFor(() => {
			expect(mockState.copyFolder).toHaveBeenCalledWith(10, 20);
		});
		expect(mockState.store.refresh).toHaveBeenCalledTimes(2);
	});

	it("opens the share dialog with the mode implied by the chosen menu entry", async () => {
		render(<FileBrowserPage />);

		fireEvent.click(screen.getByRole("button", { name: "share-folder" }));
		expect(await screen.findByText("share:Docs A:page")).toBeInTheDocument();

		fireEvent.click(screen.getByRole("button", { name: "share-file-page" }));
		expect(
			await screen.findByText("share:report.pdf:page"),
		).toBeInTheDocument();

		fireEvent.click(screen.getByRole("button", { name: "share-file-direct" }));
		expect(
			await screen.findByText("share:report.pdf:direct"),
		).toBeInTheDocument();
	});

	it("refreshes the current list after a share is created", async () => {
		render(<FileBrowserPage />);

		fireEvent.click(screen.getByRole("button", { name: "share-file-page" }));
		fireEvent.click(
			await screen.findByRole("button", {
				name: "create-share-success",
			}),
		);

		await waitFor(() => {
			expect(mockState.store.refresh).toHaveBeenCalledTimes(1);
		});
	});

	it("does not refresh when opening a direct-link share dialog", async () => {
		render(<FileBrowserPage />);

		fireEvent.click(screen.getByRole("button", { name: "share-file-direct" }));
		expect(
			await screen.findByText("share:report.pdf:direct"),
		).toBeInTheDocument();

		expect(mockState.store.refresh).not.toHaveBeenCalled();
	});

	it("keeps the share dialog flow alive when the post-create refresh fails", async () => {
		render(<FileBrowserPage />);

		fireEvent.click(screen.getByRole("button", { name: "share-file-page" }));
		fireEvent.click(
			await screen.findByRole("button", {
				name: "create-share-refresh-fails",
			}),
		);

		await waitFor(() => {
			expect(mockState.store.refresh).toHaveBeenCalledTimes(1);
		});
		expect(screen.getByText("share:report.pdf:page")).toBeInTheDocument();
	});

	it("starts a streamed archive download from a folder action", async () => {
		render(<FileBrowserPage />);

		fireEvent.click(screen.getByRole("button", { name: "archive-folder" }));

		expect(mockState.streamArchiveDownload).toHaveBeenCalledWith([], [5]);
		expect(mockState.toastSuccess).not.toHaveBeenCalled();
	});

	it("opens a naming dialog for batch archive compress and clears selection after task creation", async () => {
		render(<FileBrowserPage />);

		fireEvent.click(
			screen.getByRole("button", { name: "batch-archive-compress" }),
		);

		expect(
			await screen.findByText("archive-dialog:compress:notes.txt.zip"),
		).toBeInTheDocument();

		fireEvent.click(
			screen.getByRole("button", { name: "confirm-archive-dialog-custom" }),
		);

		await waitFor(() => {
			expect(mockState.createArchiveCompressTask).toHaveBeenCalledWith(
				[3],
				[],
				"custom-bundle.zip",
			);
		});
		expect(mockState.store.clearSelection).toHaveBeenCalledTimes(1);
		expect(mockState.toastSuccess).toHaveBeenCalledWith(
			"tasks:task_created_success",
			{
				description: "Compress custom-bundle.zip",
			},
		);
	});

	it("opens a naming dialog for archive extract and submits the custom output folder", async () => {
		mockState.store.files = [createFile({ id: 3, name: "bundle.zip" })];

		render(<FileBrowserPage />);

		const context = getFileBrowserContext();
		context.onArchiveExtract(3);

		expect(
			await screen.findByText("archive-dialog:extract:bundle"),
		).toBeInTheDocument();

		fireEvent.click(
			screen.getByRole("button", { name: "confirm-archive-dialog-custom" }),
		);

		await waitFor(() => {
			expect(mockState.createArchiveExtractTask).toHaveBeenCalledWith(
				3,
				undefined,
				"custom-output",
				undefined,
			);
		});
		expect(mockState.store.clearSelection).not.toHaveBeenCalled();
		expect(mockState.toastSuccess).toHaveBeenCalledWith(
			"tasks:task_created_success",
			{
				description: "Extract bundle.zip",
			},
		);
	});

	it("re-observes infinite scroll when pagination becomes available after the first render", async () => {
		const originalIntersectionObserver = window.IntersectionObserver;
		Object.defineProperty(window, "IntersectionObserver", {
			writable: true,
			value: MockIntersectionObserver,
		});

		try {
			mockState.store.hasMoreFiles.mockReturnValue(false);

			const { container, rerender } = render(<FileBrowserPage />);
			expect(MockIntersectionObserver.instances).toHaveLength(0);

			mockState.store.hasMoreFiles.mockReturnValue(true);
			rerender(<FileBrowserPage />);

			await waitFor(() => {
				expect(MockIntersectionObserver.instances).toHaveLength(1);
			});

			const observer = MockIntersectionObserver.instances[0];
			const target = observer?.observe.mock.calls[0]?.[0] as
				| Element
				| undefined;
			expect(target).toBeInstanceOf(HTMLElement);

			if (observer && target) {
				observer.trigger(target);
			}

			await waitFor(() => {
				expect(mockState.store.loadMoreFiles).toHaveBeenCalledTimes(1);
			});
			expect(container.querySelector(".flex.justify-center.py-4")).toBeTruthy();
		} finally {
			Object.defineProperty(window, "IntersectionObserver", {
				writable: true,
				value: originalIntersectionObserver,
			});
		}
	});

	it("moves items, publishes storage updates, and shows the formatted move toast", async () => {
		const { subscribeStorageChange } = await import("@/lib/storageChangeBus");
		const storageEvents: unknown[] = [];
		const unsubscribe = subscribeStorageChange((event) => {
			storageEvents.push(event);
		});

		vi.useFakeTimers();
		try {
			render(<FileBrowserPage />);

			fireEvent.click(screen.getByRole("button", { name: "move-selection" }));

			await Promise.resolve();
			await Promise.resolve();
			expect(mockState.store.moveToFolder).toHaveBeenCalledWith([7], [8], 20);
			await vi.advanceTimersByTimeAsync(FILE_BROWSER_FEEDBACK_DURATION_MS);
			await Promise.resolve();
			await Promise.resolve();

			expect(storageEvents).toContainEqual(
				expect.objectContaining({
					folder_ids: [8],
					kind: "folder.updated",
				}),
			);
			expect(mockState.toastSuccess).toHaveBeenCalledWith("move:ok", {
				description: "move:desc",
			});
		} finally {
			unsubscribe();
			vi.useRealTimers();
		}
	});

	it("handles trash drops via the layout and refreshes selection and user state", async () => {
		render(<FileBrowserPage />);

		vi.useFakeTimers();

		fireEvent.click(screen.getByRole("button", { name: "layout-trash" }));

		await Promise.resolve();
		await Promise.resolve();
		expect(mockState.batchDelete).toHaveBeenCalledWith([1], [2]);
		await vi.advanceTimersByTimeAsync(FILE_BROWSER_FEEDBACK_DURATION_MS);
		await Promise.resolve();
		await Promise.resolve();
		vi.useRealTimers();

		expect(mockState.store.clearSelection).toHaveBeenCalledTimes(1);
		expect(mockState.store.refresh).toHaveBeenCalledTimes(1);
		expect(mockState.refreshUser).not.toHaveBeenCalled();
		expect(mockState.toastSuccess).toHaveBeenCalledWith("delete:ok", {
			description: "delete:desc",
		});
		expect(
			consumeStorageEventEcho({
				kind: "file.trashed",
				workspace: { kind: "personal" },
				file_ids: [1],
				folder_ids: [],
				affected_parent_ids: [12],
				root_affected: false,
				affects_quota: false,
				storage_delta: null,
				at: "2026-05-13T00:00:00Z",
			}),
		).toBe(true);
		expect(
			consumeStorageEventEcho({
				kind: "folder.trashed",
				workspace: { kind: "personal" },
				file_ids: [],
				folder_ids: [2],
				affected_parent_ids: [12],
				root_affected: false,
				affects_quota: false,
				storage_delta: null,
				at: "2026-05-13T00:00:00Z",
			}),
		).toBe(true);
	});

	it("restores preview state from search navigation and supports preview callbacks", async () => {
		const previewFile = createFile({
			id: 31,
			name: "from-search.txt",
		});
		mockState.location = {
			pathname: "/folder/12",
			search: "?name=Projects",
			state: {
				searchPreviewFile: previewFile,
			},
		};

		render(<FileBrowserPage />);

		expect(
			await screen.findByText("preview:from-search.txt:auto"),
		).toBeInTheDocument();
		expect(mockState.navigate).toHaveBeenCalledWith(
			{
				pathname: "/folder/12",
				search: "?name=Projects",
			},
			{
				replace: true,
				state: null,
			},
		);

		fireEvent.click(
			screen.getByRole("button", { name: "refresh-preview-file" }),
		);
		fireEvent.click(
			screen.getByRole("button", { name: "resolve-preview-resource" }),
		);
		fireEvent.click(
			screen.getByRole("button", { name: "get-archive-preview" }),
		);
		fireEvent.click(
			screen.getByRole("button", { name: "create-wopi-session" }),
		);

		expect(mockState.store.refresh).toHaveBeenCalledTimes(1);
		expect(mockState.resolveResourceHandle).toHaveBeenCalledWith(31, {
			delivery_mode: "blob_url",
		});
		expect(mockState.createPreviewLink).not.toHaveBeenCalled();
		expect(mockState.getArchivePreview).toHaveBeenCalledWith(31, undefined);
		expect(mockState.createWopiSession).toHaveBeenCalledWith(31, "office");

		fireEvent.click(screen.getByRole("button", { name: "close-preview" }));
		expect(
			screen.queryByText("preview:from-search.txt:auto"),
		).not.toBeInTheDocument();
	});

	it("handles rename requests and browser item actions through the file browser context", async () => {
		render(<FileBrowserPage />);

		await waitFor(() => {
			expect(mockState.store.navigateTo).toHaveBeenCalledWith(12, "Projects");
		});

		document.body.dispatchEvent(
			new CustomEvent("rename-request", {
				bubbles: true,
				detail: {
					type: "file",
					id: 3,
					name: "renamed-from-event.txt",
				},
			}),
		);
		expect(
			await screen.findByText("rename:renamed-from-event.txt"),
		).toBeInTheDocument();

		const context = getFileBrowserContext();

		context.onDownload(3, "notes.txt");
		expect(mockState.startAuthenticatedDownload).toHaveBeenCalledWith(
			"/files/3/download",
		);

		await expect(context.onToggleLock("file", 3, false)).resolves.toBe(true);
		await expect(context.onToggleLock("folder", 5, true)).resolves.toBe(true);
		expect(mockState.setFileLock).toHaveBeenCalledWith(3, true);
		expect(mockState.setFolderLock).toHaveBeenCalledWith(5, false);
		expect(mockState.toastSuccess).toHaveBeenCalledWith("lock_success");
		expect(mockState.toastSuccess).toHaveBeenCalledWith("unlock_success");

		const lockError = new Error("lock failed");
		mockState.setFileLock.mockRejectedValueOnce(lockError);
		await expect(context.onToggleLock("file", 9, false)).resolves.toBe(false);
		expect(mockState.handleApiError).toHaveBeenCalledWith(lockError);

		await context.onDelete("file", 3);
		await context.onDelete("folder", 5);
		expect(mockState.store.deleteFile).toHaveBeenCalledWith(3);
		expect(mockState.store.deleteFolder).toHaveBeenCalledWith(5);
		expect(mockState.toastSuccess).toHaveBeenCalledWith("delete_success");

		const deleteError = new Error("delete failed");
		mockState.store.deleteFile.mockRejectedValueOnce(deleteError);
		await context.onDelete("file", 3);
		expect(mockState.handleApiError).toHaveBeenCalledWith(deleteError);

		act(() => {
			context.onMove("file", 3);
		});
		expect(await screen.findByText("batch-dialog:move")).toBeInTheDocument();
		fireEvent.click(
			screen.getByRole("button", { name: "close-batch-dialog:move" }),
		);
		expect(screen.queryByText("batch-dialog:move")).not.toBeInTheDocument();

		act(() => {
			context.onVersions(3);
		});
		expect(
			await screen.findByText("version-history-dialog"),
		).toBeInTheDocument();
		fireEvent.click(
			screen.getByRole("button", { name: "restore-version-history" }),
		);
		await waitFor(() => {
			expect(mockState.store.refresh).toHaveBeenCalled();
		});
	});

	it("updates open info panels when the backing file or folder changes", async () => {
		const { rerender } = render(<FileBrowserPage />);

		const context = getFileBrowserContext();

		act(() => {
			context.onInfo("file", 3);
		});
		expect(await screen.findByText("info:notes.txt")).toBeInTheDocument();

		mockState.store.files = [createFile({ id: 3, name: "notes-updated.txt" })];
		rerender(<FileBrowserPage />);
		expect(
			await screen.findByText("info:notes-updated.txt"),
		).toBeInTheDocument();

		act(() => {
			context.onInfo("folder", 5);
		});
		expect(await screen.findByText("info:Docs")).toBeInTheDocument();

		mockState.store.folders = [createFolder({ id: 5, name: "Docs Updated" })];
		rerender(<FileBrowserPage />);
		expect(await screen.findByText("info:Docs Updated")).toBeInTheDocument();
	});

	it("refreshes after trash drops and skips empty archive batches", async () => {
		render(<FileBrowserPage />);

		fireEvent.click(
			screen.getByRole("button", { name: "batch-archive-empty" }),
		);
		expect(mockState.streamArchiveDownload).not.toHaveBeenCalled();

		vi.useFakeTimers();

		fireEvent.click(screen.getByRole("button", { name: "layout-trash" }));

		await Promise.resolve();
		await Promise.resolve();
		expect(mockState.batchDelete).toHaveBeenCalledWith([1], [2]);
		await vi.advanceTimersByTimeAsync(FILE_BROWSER_FEEDBACK_DURATION_MS);
		await Promise.resolve();
		await Promise.resolve();
		vi.useRealTimers();

		expect(mockState.store.refresh).toHaveBeenCalledTimes(1);
		expect(mockState.refreshUser).not.toHaveBeenCalled();
	});

	it("shows error batch toasts for move results and closes share dialogs", async () => {
		mockState.formatBatchToast.mockImplementation(() => ({
			description: "move:error-desc",
			title: "move:error",
			variant: "error",
		}));

		render(<FileBrowserPage />);

		fireEvent.click(screen.getByRole("button", { name: "share-folder" }));
		expect(await screen.findByText("share:Docs A:page")).toBeInTheDocument();
		fireEvent.click(screen.getByRole("button", { name: "close-share-dialog" }));
		expect(screen.queryByText("share:Docs A:page")).not.toBeInTheDocument();

		vi.useFakeTimers();

		fireEvent.click(screen.getByRole("button", { name: "layout-move" }));

		await Promise.resolve();
		await Promise.resolve();
		await vi.advanceTimersByTimeAsync(FILE_BROWSER_FEEDBACK_DURATION_MS);
		await Promise.resolve();
		await Promise.resolve();
		vi.useRealTimers();

		expect(mockState.toastError).toHaveBeenCalledWith("move:error", {
			description: "move:error-desc",
		});
	});
});
