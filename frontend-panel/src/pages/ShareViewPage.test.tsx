import {
	act,
	fireEvent,
	render,
	screen,
	waitFor,
} from "@testing-library/react";
import { beforeEach, describe, expect, it, vi } from "vitest";
import { FOLDER_LIMIT } from "@/lib/constants";
import ShareViewPage, {
	parseShareFolderRoute,
	SharePreviewElement,
} from "@/pages/ShareViewPage";
import { ApiError } from "@/services/http";
import { useFileStore } from "@/stores/fileStore";
import type { FileResourceHandleRequest } from "@/types/api";
import { ApiErrorCode } from "@/types/api-helpers";

const TEST_SHARE_PASSWORD = "TEST_PASSWORD";

interface CapturedPreviewFactories {
	archiveManifestLoader?: () => Promise<unknown>;
	createMediaStreamSession?: () => Promise<unknown>;
	createExternalPreviewLink?: () => Promise<unknown>;
	resolve?: (
		fileId: number,
		request: FileResourceHandleRequest,
	) => Promise<unknown>;
}

const mockState = vi.hoisted(() => ({
	capturedFileBrowserContext: null as {
		batchSelectionActions: {
			count: number;
			downloadAction?: {
				kind: "file" | "archive";
				onClick: () => void;
			};
			onArchiveCompress?: () => void;
			onCopy?: () => void;
			onDelete?: () => void;
			onManageTags?: () => void;
			onMove?: () => void;
		} | null;
		folders: Array<{ id: number; name: string }>;
		files: Array<{ id: number; name: string; mime_type: string; size: number }>;
		onFolderOpen: (id: number, name: string) => void;
		onFileClick: (file: {
			id: number;
			name: string;
			mime_type: string;
			size: number;
		}) => void;
		onDownload: (fileId: number, fileName: string) => void;
		readOnly?: boolean;
		selectionEnabled?: boolean;
	} | null,
	downloadFolderFileUrl: vi.fn(
		(token: string, fileId: number) =>
			`https://download/${token}/files/${fileId}`,
	),
	previewFactories: null as CapturedPreviewFactories | null,
	downloadFolderPath: vi.fn(
		(token: string, fileId: number) => `/s/${token}/files/${fileId}/download`,
	),
	downloadPath: vi.fn((token: string) => `/s/${token}/download`),
	createFolderFilePreviewLink: vi.fn((token: string, fileId: number) =>
		Promise.resolve({
			etag: '"etag-share"',
			expires_at: "2026-01-01T00:00:00Z",
			path: `/pv/${token}/files/${fileId}`,
		}),
	),
	imagePreviewPath: vi.fn((token: string) => `/s/${token}/image-preview`),
	folderFileImagePreviewPath: vi.fn(
		(token: string, fileId: number) =>
			`/s/${token}/files/${fileId}/image-preview`,
	),
	folderFileThumbnailPath: vi.fn(
		(token: string, fileId: number) => `/s/${token}/files/${fileId}/thumbnail`,
	),
	createStreamSession: vi.fn((token: string) =>
		Promise.resolve({
			expires_at: "2026-01-01T00:00:00Z",
			path: `/api/v1/s/${token}/stream/session/file.mp4`,
		}),
	),
	createFolderFileStreamSession: vi.fn((token: string, fileId: number) =>
		Promise.resolve({
			expires_at: "2026-01-01T00:00:00Z",
			path: `/api/v1/s/${token}/stream/session/${fileId}.mp4`,
		}),
	),
	getArchivePreview: vi.fn(() => Promise.resolve({ entries: [] })),
	getFolderFileArchivePreview: vi.fn(() => Promise.resolve({ entries: [] })),
	createPreviewLink: vi.fn((token: string) =>
		Promise.resolve({
			etag: '"etag-share"',
			expires_at: "2026-01-01T00:00:00Z",
			path: `/pv/${token}`,
		}),
	),
	getFolderFileMediaMetadata: vi.fn(() =>
		Promise.resolve({
			kind: "audio",
			metadata: {
				artist: "Folder Artist",
				has_embedded_picture: false,
				kind: "audio",
				title: "Folder Song",
			},
			status: "ready",
		}),
	),
	getMediaMetadata: vi.fn(() =>
		Promise.resolve({
			kind: "audio",
			metadata: {
				artist: "Share Artist",
				has_embedded_picture: false,
				kind: "audio",
				title: "Share Song",
			},
			status: "ready",
		}),
	),
	thumbnailPath: vi.fn((token: string) => `/s/${token}/thumbnail`),
	downloadUrl: vi.fn((token: string) => `https://download/${token}`),
	folderPagePath: vi.fn(
		(token: string, folderId: number) => `/s/${token}/folder/${folderId}`,
	),
	getSubfolderAncestors: vi.fn(),
	getInfo: vi.fn(),
	handleApiError: vi.fn(),
	listContent: vi.fn(),
	listSubfolderContent: vi.fn(),
	musicPlayTracks: vi.fn(),
	mediaDataSupportStore: {
		config: {
			enabled: true,
			kinds: {
				audio: {
					enabled: true,
					extensions: ["mp3", "flac"],
					match: "extensions",
				},
				image: { enabled: true, extensions: ["jpg"], match: "extensions" },
				video: { enabled: false, extensions: [], match: "extensions" },
			},
			max_source_bytes: 1024 * 1024 * 1024,
			version: 1,
		},
		isLoaded: true,
		load: vi.fn(async () => {}),
	},
	openWindow: vi.fn(),
	navigate: vi.fn(),
	pagePath: vi.fn((token: string) => `/s/${token}`),
	params: {
		folderId: undefined as string | undefined,
		token: "share-token" as string | undefined,
	},
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
	toastSuccess: vi.fn(),
	translate: (key: string, opts?: Record<string, unknown>) => {
		if (key === "share:expires_date") return `expires:${opts?.date}`;
		if (key === "share:n_of_m_downloads") {
			return `downloads:${opts?.count}/${opts?.max}`;
		}
		if (key === "share:n_downloads") return `downloads:${opts?.count}`;
		if (key === "share:shared_by") {
			return `shared-by:${opts?.name}`;
		}
		if (key === "share:share_content") return "share-content";
		if (key === "share:password_verified") return "password-verified";
		return key.replace(/^core:/, "");
	},
	verifyPassword: vi.fn(),
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

vi.mock("react-i18next", () => ({
	initReactI18next: {
		type: "3rdParty",
		init: vi.fn(),
	},
	useTranslation: () => ({
		t: mockState.translate,
	}),
}));

vi.mock("react-router-dom", () => ({
	useMatch: () =>
		mockState.params.folderId === undefined
			? null
			: {
					params: {
						folderId: mockState.params.folderId,
						token: mockState.params.token,
					},
				},
	useNavigate: () => mockState.navigate,
	useParams: () => mockState.params,
}));

vi.mock("sonner", () => ({
	toast: {
		success: (...args: unknown[]) => mockState.toastSuccess(...args),
	},
}));

vi.mock("@/stores/mediaDataSupportStore", () => {
	const useMediaDataSupportStore = Object.assign(
		(selector: (state: typeof mockState.mediaDataSupportStore) => unknown) =>
			selector(mockState.mediaDataSupportStore),
		{
			getState: () => mockState.mediaDataSupportStore,
		},
	);

	return { useMediaDataSupportStore };
});

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

vi.mock("@/components/common/SkeletonCard", () => ({
	SkeletonCard: () => <div>skeleton-card</div>,
}));

vi.mock("@/components/common/UserAvatarImage", () => ({
	UserAvatarImage: ({
		avatar,
		name,
	}: {
		avatar?: { url_512?: string | null; url_1024?: string | null } | null;
		name: string;
	}) => (
		<div>{`avatar:${name}:${avatar?.url_512 ?? avatar?.url_1024 ?? "none"}`}</div>
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

vi.mock("@/components/common/SortMenu", () => ({
	SortMenu: ({
		sortBy,
		sortOrder,
		onSortBy,
		onSortOrder,
	}: {
		sortBy: string;
		sortOrder: string;
		onSortBy: (value: "updated_at") => void;
		onSortOrder: (value: "desc") => void;
	}) => (
		<div>
			<span>{`sort:${sortBy}:${sortOrder}`}</span>
			<button type="button" onClick={() => onSortBy("updated_at")}>
				sort-updated
			</button>
			<button type="button" onClick={() => onSortOrder("desc")}>
				sort-desc
			</button>
		</div>
	),
}));

vi.mock("@/components/common/ViewToggle", () => ({
	ViewToggle: ({
		value,
		onChange,
	}: {
		value: string;
		onChange: (value: "grid" | "list") => void;
	}) => (
		<div>
			<div>{`view:${value}`}</div>
			<button type="button" onClick={() => onChange("grid")}>
				grid
			</button>
			<button type="button" onClick={() => onChange("list")}>
				list
			</button>
		</div>
	),
}));

vi.mock("@/components/files/FilePreview", () => ({
	FilePreview: ({
		file,
		open = true,
		resources,
		editable,
		imageNavigation,
		onClose,
	}: {
		file: { id: number; name: string };
		open?: boolean;
		resources?: {
			paths: {
				download: string;
				imagePreview?: string;
				thumbnail?: string;
			};
			resolve?: (
				fileId: number,
				request: FileResourceHandleRequest,
			) => Promise<unknown>;
			actions?: {
				loadArchiveManifest?: () => Promise<unknown>;
				createMediaStreamSession?: () => Promise<unknown>;
				createExternalPreviewLink?: () => Promise<unknown>;
			};
		};
		editable?: boolean;
		imageNavigation?: {
			nextFile?: { id: number; name: string };
			onNavigate: (file: { id: number; name: string }) => void;
			previousFile?: { id: number; name: string };
		};
		onClose?: () => void;
	}) => {
		mockState.previewFactories = {
			archiveManifestLoader: resources?.actions?.loadArchiveManifest,
			createMediaStreamSession: resources?.actions?.createMediaStreamSession,
			createExternalPreviewLink: resources?.actions?.createExternalPreviewLink,
			resolve: resources?.resolve,
		};

		return open ? (
			<div>
				<div
					data-testid="file-preview"
					data-name={file.name}
					data-download-path={resources?.paths.download ?? ""}
					data-image-preview-path={resources?.paths.imagePreview ?? ""}
					data-thumbnail-path={resources?.paths.thumbnail ?? ""}
					data-editable={String(Boolean(editable))}
					data-next-image={imageNavigation?.nextFile?.name ?? ""}
					data-previous-image={imageNavigation?.previousFile?.name ?? ""}
					data-has-archive-preview-factory={String(
						Boolean(resources?.actions?.loadArchiveManifest),
					)}
					data-has-media-stream-link-factory={String(
						Boolean(resources?.actions?.createMediaStreamSession),
					)}
				/>
				<button
					type="button"
					onClick={() => {
						void resources?.actions?.createExternalPreviewLink?.();
					}}
				>
					call-preview-link
				</button>
				<button
					type="button"
					onClick={() => {
						void resources?.actions?.loadArchiveManifest?.();
					}}
				>
					call-archive-preview
				</button>
				<button
					type="button"
					onClick={() => {
						void resources?.actions?.createMediaStreamSession?.();
					}}
				>
					call-stream-link
				</button>
				<button type="button" onClick={onClose}>
					close-preview
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
		) : null;
	},
}));

vi.mock("@/components/files/FileThumbnail", () => ({
	FileThumbnail: ({
		file,
		size,
		thumbnailPath,
	}: {
		file: { name: string };
		size?: "sm" | "lg";
		thumbnailPath?: string;
	}) => (
		<div
			data-testid="file-thumbnail"
			data-name={file.name}
			data-size={size ?? ""}
			data-thumbnail-path={thumbnailPath ?? ""}
		/>
	),
}));

vi.mock("@/components/files/FileBrowserContext", () => ({
	FileBrowserProvider: ({
		children,
		value,
	}: {
		children: React.ReactNode;
		value: typeof mockState.capturedFileBrowserContext;
	}) => {
		mockState.capturedFileBrowserContext = value;
		return <div>{children}</div>;
	},
}));

vi.mock("@/components/files/FileGrid", () => ({
	FileGrid: () => {
		const context = mockState.capturedFileBrowserContext;
		return (
			<div data-testid="file-grid">
				{context?.folders.map((folder) => (
					<button
						key={folder.id}
						type="button"
						onClick={() => context.onFolderOpen(folder.id, folder.name)}
					>
						{`folder:${folder.name}`}
					</button>
				))}
				{context?.files.map((file) => (
					<div key={file.id}>
						<span>{file.name}</span>
						<button type="button" onClick={() => context.onFileClick(file)}>
							{`preview:${file.name}`}
						</button>
						<button
							type="button"
							onClick={() => context.onDownload(file.id, file.name)}
						>
							{`download:${file.name}`}
						</button>
					</div>
				))}
			</div>
		);
	},
}));

vi.mock("@/components/files/FileTable", () => ({
	FileTable: () => {
		const context = mockState.capturedFileBrowserContext;
		return (
			<div data-testid="file-table">
				{context?.folders.map((folder) => (
					<button
						key={folder.id}
						type="button"
						onClick={() => context.onFolderOpen(folder.id, folder.name)}
					>
						{`folder:${folder.name}`}
					</button>
				))}
				{context?.files.map((file) => (
					<div key={file.id}>
						<span>{file.name}</span>
						<button type="button" onClick={() => context.onFileClick(file)}>
							{`preview:${file.name}`}
						</button>
						<button
							type="button"
							onClick={() => context.onDownload(file.id, file.name)}
						>
							{`download:${file.name}`}
						</button>
					</div>
				))}
			</div>
		);
	},
}));

vi.mock("@/components/layout/ShareTopBar", () => ({
	ShareTopBar: () => <div>share-top-bar</div>,
}));

vi.mock("@/pages/share-view/ShareFolderSidebar", () => ({
	ShareFolderSidebar: ({
		info,
		shareOwnerText,
	}: {
		info: {
			shared_by: {
				avatar: { url_512?: string | null } | null;
				name: string;
			};
		};
		shareOwnerText: string;
	}) => (
		<aside data-testid="share-folder-sidebar">
			<span>{shareOwnerText}</span>
			<span>
				avatar:{info.shared_by.name}:{info.shared_by.avatar?.url_512 ?? "none"}
			</span>
		</aside>
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
	BreadcrumbPage: ({ children }: { children: React.ReactNode }) => (
		<span>{children}</span>
	),
	BreadcrumbSeparator: () => <span>/</span>,
}));

vi.mock("@/components/ui/button", () => ({
	Button: ({
		children,
		type,
		disabled,
		onClick,
		className,
		"aria-label": ariaLabel,
		title,
	}: {
		children: React.ReactNode;
		type?: "button" | "submit";
		disabled?: boolean;
		onClick?: () => void;
		className?: string;
		"aria-label"?: string;
		title?: string;
	}) => (
		<button
			type={type ?? "button"}
			disabled={disabled}
			onClick={onClick}
			className={className}
			aria-label={ariaLabel}
			title={title}
		>
			{children}
		</button>
	),
}));

vi.mock("@/components/ui/card", () => ({
	Card: ({
		children,
		className,
	}: {
		children: React.ReactNode;
		className?: string;
	}) => <div className={className}>{children}</div>,
	CardContent: ({ children }: { children: React.ReactNode }) => (
		<div>{children}</div>
	),
	CardDescription: ({ children }: { children: React.ReactNode }) => (
		<p>{children}</p>
	),
	CardHeader: ({
		children,
		className,
	}: {
		children: React.ReactNode;
		className?: string;
	}) => <div className={className}>{children}</div>,
	CardTitle: ({
		children,
		className,
	}: {
		children: React.ReactNode;
		className?: string;
	}) => <h2 className={className}>{children}</h2>,
}));

vi.mock("@/components/ui/icon", () => ({
	Icon: ({ name }: { name: string }) => <span>{name}</span>,
}));

vi.mock("@/components/ui/input", () => ({
	Input: ({ ...props }: React.InputHTMLAttributes<HTMLInputElement>) => (
		<input {...props} />
	),
}));

vi.mock("@/hooks/useApiError", () => ({
	handleApiError: (...args: unknown[]) => mockState.handleApiError(...args),
}));

vi.mock("@/lib/format", () => ({
	formatBytes: (value: number) => `${value} B`,
	formatDateShort: (value: string) => `fmt:${value}`,
	formatNumber: (value: number) => `${value}`,
}));

vi.mock("@/services/shareService", () => ({
	shareService: {
		downloadFolderFileUrl: (...args: unknown[]) =>
			mockState.downloadFolderFileUrl(...args),
		downloadFolderPath: (...args: unknown[]) =>
			mockState.downloadFolderPath(...args),
		downloadPath: (...args: unknown[]) => mockState.downloadPath(...args),
		createStreamSession: (...args: unknown[]) =>
			mockState.createStreamSession(...args),
		createFolderFileStreamSession: (...args: unknown[]) =>
			mockState.createFolderFileStreamSession(...args),
		createPreviewLink: (...args: unknown[]) =>
			mockState.createPreviewLink(...args),
		createFolderFilePreviewLink: (...args: unknown[]) =>
			mockState.createFolderFilePreviewLink(...args),
		getArchivePreview: (...args: unknown[]) =>
			mockState.getArchivePreview(...args),
		getFolderFileArchivePreview: (...args: unknown[]) =>
			mockState.getFolderFileArchivePreview(...args),
		getMediaMetadata: (...args: unknown[]) =>
			mockState.getMediaMetadata(...args),
		getFolderFileMediaMetadata: (...args: unknown[]) =>
			mockState.getFolderFileMediaMetadata(...args),
		thumbnailPath: (...args: unknown[]) => mockState.thumbnailPath(...args),
		folderFileThumbnailPath: (...args: unknown[]) =>
			mockState.folderFileThumbnailPath(...args),
		imagePreviewPath: (...args: unknown[]) =>
			mockState.imagePreviewPath(...args),
		folderFileImagePreviewPath: (...args: unknown[]) =>
			mockState.folderFileImagePreviewPath(...args),
		downloadUrl: (...args: unknown[]) => mockState.downloadUrl(...args),
		folderPagePath: (...args: unknown[]) => mockState.folderPagePath(...args),
		getSubfolderAncestors: (...args: unknown[]) =>
			mockState.getSubfolderAncestors(...args),
		getInfo: (...args: unknown[]) => mockState.getInfo(...args),
		listContent: (...args: unknown[]) => mockState.listContent(...args),
		listSubfolderContent: (...args: unknown[]) =>
			mockState.listSubfolderContent(...args),
		pagePath: (...args: unknown[]) => mockState.pagePath(...args),
		verifyPassword: (...args: unknown[]) => mockState.verifyPassword(...args),
	},
}));

describe("ShareViewPage", () => {
	beforeEach(() => {
		mockState.capturedFileBrowserContext = null;
		useFileStore.getState().clearSelection();
		mockState.downloadFolderFileUrl.mockClear();
		mockState.previewFactories = null;
		mockState.createFolderFilePreviewLink.mockClear();
		mockState.downloadFolderPath.mockClear();
		mockState.downloadPath.mockClear();
		mockState.createStreamSession.mockClear();
		mockState.createFolderFileStreamSession.mockClear();
		mockState.createPreviewLink.mockClear();
		mockState.getArchivePreview.mockClear();
		mockState.getFolderFileArchivePreview.mockClear();
		mockState.getMediaMetadata.mockClear();
		mockState.getFolderFileMediaMetadata.mockClear();
		mockState.thumbnailPath.mockClear();
		mockState.folderFileThumbnailPath.mockClear();
		mockState.imagePreviewPath.mockClear();
		mockState.folderFileImagePreviewPath.mockClear();
		mockState.downloadUrl.mockClear();
		mockState.folderPagePath.mockClear();
		mockState.getSubfolderAncestors.mockReset();
		mockState.getInfo.mockReset();
		mockState.handleApiError.mockReset();
		MockIntersectionObserver.reset();
		mockState.listContent.mockReset();
		mockState.listSubfolderContent.mockReset();
		mockState.mediaDataSupportStore.isLoaded = true;
		mockState.mediaDataSupportStore.load.mockReset();
		mockState.mediaDataSupportStore.load.mockResolvedValue(undefined);
		mockState.openWindow.mockReset();
		mockState.navigate.mockReset();
		mockState.pagePath.mockClear();
		mockState.params = { folderId: undefined, token: "share-token" };
		mockState.previewAppStore.load.mockReset();
		mockState.previewAppStore.isLoaded = false;
		mockState.previewAppStore.load.mockResolvedValue(undefined);
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
		mockState.thumbnailSupportStore.load.mockResolvedValue(undefined);
		mockState.toastSuccess.mockReset();
		mockState.verifyPassword.mockReset();
		mockState.musicPlayTracks.mockReset();
		mockState.verifyPassword.mockResolvedValue(undefined);
		mockState.listContent.mockResolvedValue({
			files: [],
			folders: [],
			next_file_cursor: null,
		} as never);
		mockState.listSubfolderContent.mockResolvedValue({
			files: [],
			folders: [],
			next_file_cursor: null,
		} as never);

		Object.defineProperty(window, "open", {
			configurable: true,
			value: mockState.openWindow,
		});
	});

	it("renders an unavailable panel for expired shares", async () => {
		mockState.getInfo.mockRejectedValueOnce(
			new ApiError(ApiErrorCode.ShareExpired, "expired"),
		);

		render(<ShareViewPage />);

		expect(await screen.findByText("errors:share_expired")).toBeInTheDocument();
		expect(screen.getByText("unavailable")).toBeInTheDocument();
	});

	it.each([
		"0",
		"-1",
		"abc",
		"1.5",
		"01",
		"9007199254740992",
	])("rejects invalid share folder route %s without issuing API requests", async (folderId) => {
		mockState.params = { folderId, token: "share-token" };

		render(<ShareViewPage />);

		expect(
			await screen.findByText("errors:folder_not_found"),
		).toBeInTheDocument();
		expect(mockState.getInfo).not.toHaveBeenCalled();
		expect(mockState.listContent).not.toHaveBeenCalled();
		expect(mockState.listSubfolderContent).not.toHaveBeenCalled();
	});

	it("parses only canonical positive safe folder ids", () => {
		expect(parseShareFolderRoute(undefined)).toBeNull();
		expect(parseShareFolderRoute("42")).toBe(42);
		expect(parseShareFolderRoute(String(Number.MAX_SAFE_INTEGER))).toBe(
			Number.MAX_SAFE_INTEGER,
		);
		expect(parseShareFolderRoute("9007199254740992")).toBeUndefined();
	});

	it("does not render retained preview resources while share info is unavailable", () => {
		render(
			<SharePreviewElement
				info={null}
				token="share-token"
				previewFile={
					{
						id: 77,
						mime_type: "audio/mpeg",
						name: "orphaned-preview.mp3",
						size: 77,
					} as never
				}
				onClose={vi.fn()}
				onPreviewNavigate={vi.fn()}
			/>,
		);

		expect(screen.queryByTestId("file-preview")).not.toBeInTheDocument();
		expect(mockState.previewFactories).toBeNull();
	});

	it("maps share load errors to the public unavailable panel", async () => {
		mockState.getInfo.mockRejectedValueOnce(
			new ApiError(ApiErrorCode.ShareNotFound, "missing"),
		);

		const { unmount } = render(<ShareViewPage />);

		expect(
			await screen.findByText("errors:share_not_found"),
		).toBeInTheDocument();
		unmount();

		mockState.getInfo.mockRejectedValueOnce(
			new ApiError(
				ApiErrorCode.ShareDownloadLimitReached,
				"download limit reached",
			),
		);

		const limited = render(<ShareViewPage />);

		expect(
			await screen.findByText("share:download_limit_reached"),
		).toBeInTheDocument();
		limited.unmount();

		mockState.getInfo.mockRejectedValueOnce(
			new ApiError(ApiErrorCode.BadRequest, "bad request"),
		);

		const badRequest = render(<ShareViewPage />);

		expect(await screen.findByText("bad request")).toBeInTheDocument();
		badRequest.unmount();

		mockState.getInfo.mockRejectedValueOnce(new Error("network down"));

		render(<ShareViewPage />);

		expect(
			await screen.findByText("share:failed_to_load_share"),
		).toBeInTheDocument();
	});

	it("verifies passwords for protected folder shares and then loads their contents", async () => {
		mockState.getInfo.mockResolvedValueOnce({
			has_password: true,
			name: "Secret Folder",
			shared_by: {
				avatar: {
					source: "upload",
					url_512: "/s/share-token/avatar/512?v=1",
					url_1024: "/s/share-token/avatar/1024?v=1",
					version: 1,
				},
				name: "Alice Example",
			},
			share_type: "folder",
		} as never);
		mockState.listContent
			.mockRejectedValueOnce(
				new ApiError(ApiErrorCode.SharePasswordRequired, "password required"),
			)
			.mockResolvedValueOnce({
				files: [],
				folders: [{ id: 1, name: "Docs" }],
				next_file_cursor: null,
			} as never);

		render(<ShareViewPage />);

		await screen.findByText("Secret Folder");
		expect(mockState.previewAppStore.load).toHaveBeenCalledTimes(1);
		const passwordInput = screen.getByLabelText("password");
		expect(passwordInput).toHaveAttribute("id", "share-password");
		expect(passwordInput).toHaveAttribute("autocomplete", "current-password");
		expect(passwordInput).not.toHaveAttribute("autofocus");
		fireEvent.change(passwordInput, {
			target: { value: TEST_SHARE_PASSWORD },
		});
		fireEvent.click(screen.getByRole("button", { name: "verify" }));

		await waitFor(() => {
			expect(mockState.verifyPassword).toHaveBeenCalledWith("share-token", {
				password: TEST_SHARE_PASSWORD,
			});
		});
		expect(mockState.toastSuccess).toHaveBeenCalledWith("password-verified");
		expect(mockState.listContent).toHaveBeenCalledWith("share-token", {
			file_limit: 100,
			folder_limit: FOLDER_LIMIT,
		});
		expect(
			await screen.findByRole("button", { name: "folder:Docs" }),
		).toBeInTheDocument();
		expect(screen.getByText("Secret Folder")).toBeInTheDocument();
		expect(screen.getByText("shared-by:Alice Example")).toBeInTheDocument();
		expect(screen.queryByText("share-content")).not.toBeInTheDocument();
	});

	it("keeps the password panel open and reports verify failures", async () => {
		mockState.getInfo.mockResolvedValueOnce({
			has_password: true,
			name: "Secret Folder",
			shared_by: {
				avatar: null,
				name: "Alice Example",
			},
			share_type: "folder",
		} as never);
		const error = new ApiError(
			ApiErrorCode.CredentialsFailed,
			"wrong password",
		);
		mockState.verifyPassword.mockRejectedValueOnce(error);
		mockState.listContent.mockRejectedValueOnce(
			new ApiError(ApiErrorCode.SharePasswordRequired, "password required"),
		);

		render(<ShareViewPage />);

		const passwordInput = await screen.findByLabelText("password");
		fireEvent.change(passwordInput, {
			target: { value: "wrong" },
		});
		fireEvent.click(screen.getByRole("button", { name: "verify" }));

		await waitFor(() => {
			expect(mockState.handleApiError).toHaveBeenCalledWith(error);
		});
		expect(screen.getByLabelText("password")).toHaveValue("wrong");
		expect(mockState.listContent).toHaveBeenCalledTimes(1);
		expect(screen.queryByText("share-content")).not.toBeInTheDocument();
	});

	it("resets password verification and input when switching protected shares", async () => {
		mockState.getInfo
			.mockResolvedValueOnce({
				has_password: true,
				name: "First Secret",
				shared_by: {
					avatar: null,
					name: "Alice Example",
				},
				share_type: "folder",
			} as never)
			.mockResolvedValueOnce({
				has_password: true,
				name: "Second Secret",
				shared_by: {
					avatar: null,
					name: "Bob Example",
				},
				share_type: "folder",
			} as never);
		mockState.listContent
			.mockRejectedValueOnce(
				new ApiError(ApiErrorCode.SharePasswordRequired, "password required"),
			)
			.mockResolvedValueOnce({
				files: [],
				folders: [{ id: 1, name: "Docs" }],
				next_file_cursor: null,
			} as never)
			.mockRejectedValueOnce(
				new ApiError(ApiErrorCode.SharePasswordRequired, "password required"),
			);

		const { rerender } = render(<ShareViewPage />);

		const passwordInput = await screen.findByLabelText("password");
		fireEvent.change(passwordInput, {
			target: { value: "first-password" },
		});
		fireEvent.click(screen.getByRole("button", { name: "verify" }));

		await waitFor(() => {
			expect(mockState.verifyPassword).toHaveBeenCalledWith("share-token", {
				password: "first-password",
			});
		});
		expect(
			await screen.findByRole("button", { name: "folder:Docs" }),
		).toBeInTheDocument();

		mockState.params = { folderId: undefined, token: "second-token" };
		rerender(<ShareViewPage />);

		await waitFor(() => {
			expect(mockState.getInfo).toHaveBeenCalledWith("second-token");
		});
		const nextPasswordInput = await screen.findByLabelText("password");
		expect(nextPasswordInput).toHaveValue("");
		expect(screen.getByText("Second Secret")).toBeInTheDocument();
		expect(
			screen.queryByRole("button", { name: "folder:Docs" }),
		).not.toBeInTheDocument();
	});

	it("renders file shares with preview and download actions", async () => {
		mockState.getInfo.mockResolvedValueOnce({
			download_count: 3,
			expires_at: "2026-04-01T00:00:00Z",
			has_password: false,
			max_downloads: 5,
			mime_type: "application/pdf",
			name: "Manual.pdf",
			shared_by: {
				avatar: {
					source: "upload",
					url_512: "/s/share-token/avatar/512?v=1",
					url_1024: "/s/share-token/avatar/1024?v=1",
					version: 1,
				},
				name: "Alice Example",
			},
			share_type: "file",
			size: 256,
		} as never);

		render(<ShareViewPage />);

		expect(await screen.findByText("Manual.pdf")).toBeInTheDocument();
		const metadata = screen.getByText("Manual.pdf").parentElement;
		expect(metadata).toHaveTextContent("downloads:3/5");
		expect(metadata).toHaveTextContent("expires:fmt:2026-04-01T00:00:00Z");
		expect(screen.getByText("shared-by:Alice Example")).toBeInTheDocument();
		expect(
			screen.getByText("avatar:Alice Example:/s/share-token/avatar/512?v=1"),
		).toBeInTheDocument();
		expect(screen.getByTestId("file-thumbnail")).toHaveAttribute(
			"data-name",
			"Manual.pdf",
		);
		expect(screen.getByTestId("file-thumbnail")).toHaveAttribute(
			"data-size",
			"lg",
		);
		expect(screen.getByTestId("file-thumbnail")).toHaveAttribute(
			"data-thumbnail-path",
			"/s/share-token/thumbnail",
		);
		expect(mockState.thumbnailPath).toHaveBeenCalledWith("share-token");

		fireEvent.click(screen.getByRole("button", { name: /files:preview/i }));

		expect(await screen.findByTestId("file-preview")).toHaveAttribute(
			"data-download-path",
			"/s/share-token/download",
		);
		fireEvent.click(screen.getByRole("button", { name: "close-preview" }));
		expect(screen.queryByTestId("file-preview")).not.toBeInTheDocument();
		fireEvent.click(screen.getByRole("button", { name: /files:preview/i }));
		expect(await screen.findByTestId("file-preview")).toHaveAttribute(
			"data-download-path",
			"/s/share-token/download",
		);
		expect(screen.getByTestId("file-preview")).toHaveAttribute(
			"data-thumbnail-path",
			"/s/share-token/thumbnail",
		);
		expect(screen.getByTestId("file-preview")).toHaveAttribute(
			"data-editable",
			"false",
		);
		expect(screen.getByTestId("file-preview")).toHaveAttribute(
			"data-has-archive-preview-factory",
			"true",
		);
		expect(screen.getByTestId("file-preview")).toHaveAttribute(
			"data-has-media-stream-link-factory",
			"true",
		);

		fireEvent.click(screen.getByRole("button", { name: "call-preview-link" }));
		fireEvent.click(
			screen.getByRole("button", { name: "call-archive-preview" }),
		);
		fireEvent.click(screen.getByRole("button", { name: "call-stream-link" }));

		await waitFor(() => {
			expect(mockState.createPreviewLink).toHaveBeenCalledWith("share-token");
			expect(mockState.getArchivePreview).toHaveBeenCalledWith(
				"share-token",
				undefined,
			);
			expect(mockState.createStreamSession).toHaveBeenCalledWith("share-token");
		});
		expect(mockState.getMediaMetadata).not.toHaveBeenCalled();
		await expect(
			mockState.previewFactories?.resolve?.(-1, {
				delivery_mode: "direct_url",
				representation: "original",
			}),
		).resolves.toMatchObject({
			identity: {
				cacheKey: "/s/share-token/download",
				scope: "share",
			},
			request: {
				url: "/s/share-token/download",
			},
			delivery: {
				mimeType: "application/pdf",
				mode: "direct_url",
			},
		});
		await expect(
			mockState.previewFactories?.resolve?.(-1, {
				delivery_mode: "blob_url",
				representation: "thumbnail",
			}),
		).resolves.toMatchObject({
			identity: {
				cacheKey: "/s/share-token/thumbnail",
				scope: "share",
			},
			request: {
				url: "/s/share-token/thumbnail",
			},
			delivery: {
				mimeType: "image/webp",
				mode: "blob_url",
			},
		});

		fireEvent.click(screen.getByRole("button", { name: /files:download/i }));

		expect(mockState.downloadUrl).toHaveBeenCalledWith("share-token");
		expect(mockState.openWindow).toHaveBeenCalledWith(
			"https://download/share-token",
			"_blank",
			"noopener,noreferrer",
		);
	});

	it("renders file shares without preview when mime_type is missing", async () => {
		mockState.getInfo.mockResolvedValueOnce({
			download_count: 0,
			has_password: false,
			name: "Unknown.bin",
			shared_by: {
				avatar: null,
				name: "Alice Example",
			},
			share_type: "file",
		} as never);

		render(<ShareViewPage />);

		expect(await screen.findByText("Unknown.bin")).toBeInTheDocument();
		expect(screen.getByText("File")).toBeInTheDocument();
		expect(screen.queryByTestId("file-thumbnail")).not.toBeInTheDocument();
		expect(
			screen.queryByRole("button", { name: /files:preview/i }),
		).not.toBeInTheDocument();

		fireEvent.click(screen.getByRole("button", { name: /files:download/i }));

		expect(mockState.downloadUrl).toHaveBeenCalledWith("share-token");
		expect(mockState.openWindow).toHaveBeenCalledWith(
			"https://download/share-token",
			"_blank",
			"noopener,noreferrer",
		);
	});

	it("does not bootstrap media metadata support from the preview element", async () => {
		mockState.mediaDataSupportStore.isLoaded = false;
		mockState.getInfo.mockResolvedValueOnce({
			download_count: 0,
			has_password: false,
			mime_type: "application/pdf",
			name: "Manual.pdf",
			shared_by: {
				avatar: null,
				name: "Alice Example",
			},
			share_type: "file",
			size: 256,
		} as never);

		render(<ShareViewPage />);

		expect(await screen.findByText("Manual.pdf")).toBeInTheDocument();
		expect(mockState.mediaDataSupportStore.load).not.toHaveBeenCalled();
	});

	it("falls back to file preview for shared audio without preview metadata loaders", async () => {
		mockState.getInfo.mockResolvedValueOnce({
			download_count: 0,
			has_password: false,
			mime_type: "audio/mpeg",
			name: "Preview Song.mp3",
			shared_by: {
				avatar: null,
				name: "Alice Example",
			},
			share_type: "file",
			size: 512,
		} as never);
		mockState.musicPlayTracks.mockImplementationOnce(() => {
			throw new Error("fall back to preview");
		});

		render(<ShareViewPage />);

		await screen.findByText("Preview Song.mp3");
		fireEvent.click(screen.getByRole("button", { name: /files:preview/i }));

		expect(await screen.findByTestId("file-preview")).toHaveAttribute(
			"data-download-path",
			"/s/share-token/download",
		);
		expect(mockState.getMediaMetadata).not.toHaveBeenCalled();
	});

	it("plays shared music directly without opening the file preview", async () => {
		mockState.getInfo.mockResolvedValueOnce({
			download_count: 0,
			has_password: false,
			mime_type: "audio/mpeg",
			name: "Shared Song.mp3",
			shared_by: {
				avatar: null,
				name: "Alice Example",
			},
			share_type: "file",
			size: 512,
		} as never);

		render(<ShareViewPage />);

		await screen.findByText("Shared Song.mp3");
		fireEvent.click(screen.getByRole("button", { name: /files:preview/i }));

		await waitFor(() => {
			expect(mockState.musicPlayTracks).toHaveBeenCalledWith(
				[
					expect.objectContaining({
						id: "share:share-token:file",
						name: "Shared Song.mp3",
					}),
				],
				"share:share-token:file",
			);
		});
		expect(screen.queryByTestId("file-preview")).not.toBeInTheDocument();
	});

	it("navigates folder shares and uses the folder-specific preview and download paths", async () => {
		mockState.getInfo.mockResolvedValue({
			has_password: false,
			name: "Shared Root",
			shared_by: {
				avatar: {
					source: "gravatar",
					url_512: "https://www.gravatar.com/avatar/hash?s=512",
					url_1024: "https://www.gravatar.com/avatar/hash?s=1024",
					version: 2,
				},
				name: "Alice Example",
			},
			share_type: "folder",
		} as never);
		mockState.listContent.mockResolvedValueOnce({
			files: [{ id: 2, mime_type: "text/plain", name: "root.txt", size: 2 }],
			folders: [{ id: 1, name: "Docs" }],
			next_file_cursor: null,
		} as never);
		mockState.listSubfolderContent.mockResolvedValueOnce({
			files: [{ id: 5, mime_type: "text/plain", name: "nested.txt", size: 5 }],
			folders: [],
			next_file_cursor: null,
		} as never);
		mockState.getSubfolderAncestors.mockResolvedValueOnce([
			{ id: 1, name: "Docs" },
		]);

		const { rerender } = render(<ShareViewPage />);

		expect(await screen.findByText("Shared Root")).toBeInTheDocument();
		expect(screen.getByText("shared-by:Alice Example")).toBeInTheDocument();
		expect(screen.queryByText("share-content")).not.toBeInTheDocument();
		expect(
			screen.getByText(
				"avatar:Alice Example:https://www.gravatar.com/avatar/hash?s=512",
			),
		).toBeInTheDocument();
		expect(mockState.capturedFileBrowserContext).toMatchObject({
			readOnly: true,
			selectionEnabled: true,
		});
		useFileStore.getState().selectItems([2], []);
		await waitFor(() => {
			expect(
				mockState.capturedFileBrowserContext?.batchSelectionActions,
			).toEqual(
				expect.objectContaining({
					count: 1,
					downloadAction: expect.objectContaining({ kind: "file" }),
				}),
			);
		});
		expect(
			mockState.capturedFileBrowserContext?.batchSelectionActions
				?.onArchiveCompress,
		).toBeUndefined();
		expect(
			mockState.capturedFileBrowserContext?.batchSelectionActions?.onCopy,
		).toBeUndefined();
		expect(
			mockState.capturedFileBrowserContext?.batchSelectionActions?.onDelete,
		).toBeUndefined();
		expect(
			mockState.capturedFileBrowserContext?.batchSelectionActions?.onMove,
		).toBeUndefined();
		useFileStore.getState().clearSelection();
		fireEvent.click(await screen.findByRole("button", { name: "folder:Docs" }));
		expect(mockState.navigate).toHaveBeenCalledWith("/s/share-token/folder/1");
		expect(mockState.listSubfolderContent).not.toHaveBeenCalled();

		mockState.params = { folderId: "1", token: "share-token" };
		rerender(<ShareViewPage />);

		await waitFor(() => {
			expect(mockState.listSubfolderContent).toHaveBeenCalledWith(
				"share-token",
				1,
				{
					file_limit: 100,
					folder_limit: FOLDER_LIMIT,
				},
			);
		});
		expect(await screen.findByText("nested.txt")).toBeInTheDocument();
		expect(
			await screen.findByRole("button", { name: "Shared Root" }),
		).toBeInTheDocument();
		expect(screen.getByText("Docs")).toBeInTheDocument();

		fireEvent.click(screen.getByRole("button", { name: "preview:nested.txt" }));

		expect(await screen.findByTestId("file-preview")).toHaveAttribute(
			"data-download-path",
			"/s/share-token/files/5/download",
		);
		expect(screen.getByTestId("file-preview")).toHaveAttribute(
			"data-thumbnail-path",
			"/s/share-token/files/5/thumbnail",
		);
		expect(mockState.folderFileThumbnailPath).toHaveBeenCalledWith(
			"share-token",
			5,
		);
		expect(screen.getByTestId("file-preview")).toHaveAttribute(
			"data-editable",
			"false",
		);
		expect(screen.getByTestId("file-preview")).toHaveAttribute(
			"data-has-archive-preview-factory",
			"true",
		);
		expect(screen.getByTestId("file-preview")).toHaveAttribute(
			"data-has-media-stream-link-factory",
			"true",
		);

		fireEvent.click(screen.getByRole("button", { name: "call-preview-link" }));
		fireEvent.click(
			screen.getByRole("button", { name: "call-archive-preview" }),
		);
		fireEvent.click(screen.getByRole("button", { name: "call-stream-link" }));

		await waitFor(() => {
			expect(mockState.createFolderFilePreviewLink).toHaveBeenCalledWith(
				"share-token",
				5,
			);
			expect(mockState.getFolderFileArchivePreview).toHaveBeenCalledWith(
				"share-token",
				5,
				undefined,
			);
			expect(mockState.createFolderFileStreamSession).toHaveBeenCalledWith(
				"share-token",
				5,
			);
		});
		expect(mockState.getFolderFileMediaMetadata).not.toHaveBeenCalled();
		await expect(
			mockState.previewFactories?.resolve?.(5, {
				delivery_mode: "blob_url",
				representation: "auto",
			}),
		).resolves.toMatchObject({
			identity: {
				cacheKey: "/s/share-token/files/5/download",
				scope: "share",
			},
			request: {
				url: "/s/share-token/files/5/download",
			},
			delivery: {
				mimeType: "text/plain",
				mode: "blob_url",
			},
		});

		fireEvent.click(
			screen.getByRole("button", { name: "download:nested.txt" }),
		);

		expect(mockState.downloadFolderFileUrl).toHaveBeenCalledWith(
			"share-token",
			5,
		);
		expect(mockState.openWindow).toHaveBeenCalledWith(
			"https://download/share-token/files/5",
			"_blank",
			"noopener,noreferrer",
		);
	});

	it("passes adjacent image navigation to folder share previews and updates folder-specific paths", async () => {
		mockState.thumbnailSupportStore.config = {
			audio_thumbnail: { enabled: false, extensions: [] },
			extensions: ["heic", "jpg", "jpeg", "png", "webp"],
			image_preview: {
				enabled: true,
				extensions: ["heic", "jpg", "jpeg", "png", "webp"],
			},
			image_thumbnail: {
				enabled: true,
				extensions: ["heic", "jpg", "jpeg", "png", "webp"],
			},
			version: 1,
			video_thumbnail: { enabled: false, extensions: [] },
		};
		mockState.getInfo.mockResolvedValueOnce({
			has_password: false,
			name: "Shared Photos",
			shared_by: {
				avatar: null,
				name: "Alice Example",
			},
			share_type: "folder",
		} as never);
		mockState.listContent.mockResolvedValueOnce({
			files: [
				{ id: 10, mime_type: "image/png", name: "first.png", size: 10 },
				{
					id: 11,
					mime_type: "application/pdf",
					name: "notes.pdf",
					size: 11,
				},
				{ id: 12, mime_type: "image/jpeg", name: "second.jpg", size: 12 },
				{
					id: 13,
					mime_type: "image/heic",
					name: "capture.heic",
					size: 13,
				},
			],
			folders: [],
			next_file_cursor: null,
		} as never);

		render(<ShareViewPage />);

		expect(await screen.findByText("first.png")).toBeInTheDocument();
		fireEvent.click(screen.getByRole("button", { name: "preview:first.png" }));

		expect(await screen.findByTestId("file-preview")).toHaveAttribute(
			"data-name",
			"first.png",
		);
		expect(screen.getByTestId("file-preview")).toHaveAttribute(
			"data-download-path",
			"/s/share-token/files/10/download",
		);
		expect(screen.getByTestId("file-preview")).toHaveAttribute(
			"data-previous-image",
			"capture.heic",
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
			"data-download-path",
			"/s/share-token/files/12/download",
		);
		expect(screen.getByTestId("file-preview")).toHaveAttribute(
			"data-previous-image",
			"first.png",
		);
		expect(screen.getByTestId("file-preview")).toHaveAttribute(
			"data-next-image",
			"capture.heic",
		);

		fireEvent.click(screen.getByRole("button", { name: "next-image" }));
		expect(await screen.findByTestId("file-preview")).toHaveAttribute(
			"data-name",
			"capture.heic",
		);
		expect(screen.getByTestId("file-preview")).toHaveAttribute(
			"data-download-path",
			"/s/share-token/files/13/download",
		);
		expect(screen.getByTestId("file-preview")).toHaveAttribute(
			"data-previous-image",
			"second.jpg",
		);
		expect(screen.getByTestId("file-preview")).toHaveAttribute(
			"data-next-image",
			"first.png",
		);
		await expect(
			mockState.previewFactories?.resolve?.(13, {
				delivery_mode: "blob_url",
				representation: "auto",
			}),
		).resolves.toMatchObject({
			identity: {
				cacheKey: "/s/share-token/files/13/image-preview",
				scope: "share",
			},
			request: {
				url: "/s/share-token/files/13/image-preview",
			},
			delivery: {
				mimeType: "image/webp",
				mode: "blob_url",
			},
		});

		fireEvent.click(screen.getByRole("button", { name: "previous-image" }));
		expect(await screen.findByTestId("file-preview")).toHaveAttribute(
			"data-name",
			"second.jpg",
		);
	});

	it("does not expose image navigation for single-file shares", async () => {
		mockState.getInfo.mockResolvedValueOnce({
			download_count: 0,
			has_password: false,
			mime_type: "image/png",
			name: "single.png",
			shared_by: {
				avatar: null,
				name: "Alice Example",
			},
			share_type: "file",
			size: 64,
		} as never);

		render(<ShareViewPage />);

		expect(await screen.findByText("single.png")).toBeInTheDocument();
		fireEvent.click(screen.getByRole("button", { name: /files:preview/i }));

		expect(await screen.findByTestId("file-preview")).toHaveAttribute(
			"data-name",
			"single.png",
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

	it("shows a route error when subfolder loading fails without falling back to root", async () => {
		mockState.params = { folderId: "1", token: "share-token" };
		mockState.getInfo.mockResolvedValueOnce({
			has_password: false,
			name: "Shared Root",
			shared_by: {
				avatar: null,
				name: "Alice Example",
			},
			share_type: "folder",
		} as never);
		const error = new ApiError(ApiErrorCode.FolderNotFound, "missing folder");
		mockState.listSubfolderContent.mockRejectedValueOnce(error);
		mockState.getSubfolderAncestors.mockResolvedValueOnce([
			{ id: 1, name: "Docs" },
		]);

		render(<ShareViewPage />);

		expect(await screen.findByText("missing folder")).toBeInTheDocument();
		expect(mockState.listContent).not.toHaveBeenCalled();
		expect(screen.queryByText("root.txt")).not.toBeInTheDocument();
	});

	it("toggles folder view mode from grid to list", async () => {
		mockState.getInfo.mockResolvedValueOnce({
			has_password: false,
			name: "Shared Root",
			shared_by: {
				avatar: null,
				name: "Alice Example",
			},
			share_type: "folder",
		} as never);
		mockState.listContent.mockResolvedValueOnce({
			files: [],
			folders: [],
			next_file_cursor: null,
		} as never);

		render(<ShareViewPage />);

		expect(await screen.findByText("view:grid")).toBeInTheDocument();
		fireEvent.click(screen.getByRole("button", { name: "list" }));
		expect(screen.getByText("view:list")).toBeInTheDocument();
	});

	it("deduplicates repeated infinite-scroll loads for the same cursor", async () => {
		const originalIntersectionObserver = window.IntersectionObserver;
		Object.defineProperty(window, "IntersectionObserver", {
			writable: true,
			value: MockIntersectionObserver,
		});

		type LoadMoreResponse = {
			files: Array<{
				id: number;
				mime_type: string;
				name: string;
				size: number;
			}>;
			folders: [];
			next_file_cursor: null;
		};
		let resolveMore!: (contents: LoadMoreResponse) => void;
		const loadMorePromise = new Promise<LoadMoreResponse>((resolve) => {
			resolveMore = resolve;
		});

		try {
			mockState.getInfo.mockResolvedValueOnce({
				has_password: false,
				name: "Shared Root",
				shared_by: {
					avatar: null,
					name: "Alice Example",
				},
				share_type: "folder",
			} as never);
			mockState.listContent
				.mockResolvedValueOnce({
					files: [
						{ id: 1, mime_type: "text/plain", name: "first.txt", size: 1 },
					],
					folders: [],
					next_file_cursor: { id: 1, value: "first.txt" },
				} as never)
				.mockReturnValueOnce(loadMorePromise as never);

			render(<ShareViewPage />);

			expect(await screen.findByText("first.txt")).toBeInTheDocument();
			await waitFor(() => {
				expect(MockIntersectionObserver.instances).toHaveLength(1);
			});

			const observer = MockIntersectionObserver.instances[0];
			const target = observer?.observe.mock.calls[0]?.[0] as
				| Element
				| undefined;
			expect(target).toBeInstanceOf(HTMLElement);

			if (observer && target) {
				act(() => {
					observer.trigger(target);
					observer.trigger(target);
				});
			}

			await waitFor(() => {
				expect(mockState.listContent).toHaveBeenCalledTimes(2);
			});
			expect(mockState.listContent).toHaveBeenLastCalledWith("share-token", {
				file_after_id: 1,
				file_after_value: "first.txt",
				file_limit: 100,
				folder_limit: 0,
			});

			await act(async () => {
				resolveMore({
					files: [
						{ id: 2, mime_type: "text/plain", name: "second.txt", size: 2 },
					],
					folders: [],
					next_file_cursor: null,
				});
				await loadMorePromise;
			});

			expect(await screen.findByText("second.txt")).toBeInTheDocument();
			expect(mockState.listContent).toHaveBeenCalledTimes(2);
		} finally {
			Object.defineProperty(window, "IntersectionObserver", {
				writable: true,
				value: originalIntersectionObserver,
			});
		}
	});

	it("loads more files inside a subfolder and allows retry after a page error", async () => {
		const originalIntersectionObserver = window.IntersectionObserver;
		Object.defineProperty(window, "IntersectionObserver", {
			writable: true,
			value: MockIntersectionObserver,
		});

		try {
			mockState.params = { folderId: "9", token: "share-token" };
			mockState.getInfo.mockResolvedValueOnce({
				has_password: false,
				name: "Shared Root",
				shared_by: {
					avatar: null,
					name: "Alice Example",
				},
				share_type: "folder",
			} as never);
			mockState.getSubfolderAncestors.mockResolvedValueOnce([
				{ id: 9, name: "Nested" },
			]);
			mockState.listSubfolderContent
				.mockResolvedValueOnce({
					files: [
						{ id: 10, mime_type: "text/plain", name: "alpha.txt", size: 10 },
					],
					folders: [],
					next_file_cursor: { id: 10, value: "alpha.txt" },
				} as never)
				.mockRejectedValueOnce(
					new ApiError(ApiErrorCode.BadRequest, "page failed"),
				)
				.mockResolvedValueOnce({
					files: [
						{ id: 11, mime_type: "text/plain", name: "beta.txt", size: 11 },
					],
					folders: [],
					next_file_cursor: null,
				} as never);

			render(<ShareViewPage />);

			expect(await screen.findByText("alpha.txt")).toBeInTheDocument();
			await waitFor(() => {
				expect(MockIntersectionObserver.instances).toHaveLength(1);
			});

			const firstObserver = MockIntersectionObserver.instances[0];
			const firstTarget = firstObserver?.observe.mock.calls[0]?.[0] as
				| Element
				| undefined;
			expect(firstTarget).toBeInstanceOf(HTMLElement);

			if (firstObserver && firstTarget) {
				act(() => {
					firstObserver.trigger(firstTarget);
				});
			}

			await waitFor(() => {
				expect(mockState.handleApiError).toHaveBeenCalledWith(
					expect.objectContaining({ message: "page failed" }),
				);
			});
			expect(mockState.listSubfolderContent).toHaveBeenLastCalledWith(
				"share-token",
				9,
				{
					file_after_id: 10,
					file_after_value: "alpha.txt",
					file_limit: 100,
					folder_limit: 0,
				},
			);

			if (firstObserver && firstTarget) {
				act(() => {
					firstObserver.trigger(firstTarget);
				});
			}

			expect(await screen.findByText("beta.txt")).toBeInTheDocument();
			expect(mockState.listSubfolderContent).toHaveBeenCalledTimes(3);
		} finally {
			Object.defineProperty(window, "IntersectionObserver", {
				writable: true,
				value: originalIntersectionObserver,
			});
		}
	});

	it("falls back to file preview for folder audio without preview metadata loaders", async () => {
		mockState.getInfo.mockResolvedValueOnce({
			has_password: false,
			name: "Shared Root",
			shared_by: {
				avatar: null,
				name: "Alice Example",
			},
			share_type: "folder",
		} as never);
		mockState.listContent.mockResolvedValueOnce({
			files: [
				{ id: 4, mime_type: "audio/mpeg", name: "Preview Song.mp3", size: 4 },
			],
			folders: [],
			next_file_cursor: null,
		} as never);
		mockState.musicPlayTracks.mockImplementationOnce(() => {
			throw new Error("fall back to preview");
		});

		render(<ShareViewPage />);

		expect(await screen.findByText("Preview Song.mp3")).toBeInTheDocument();
		fireEvent.click(
			screen.getByRole("button", { name: "preview:Preview Song.mp3" }),
		);

		expect(await screen.findByTestId("file-preview")).toHaveAttribute(
			"data-download-path",
			"/s/share-token/files/4/download",
		);
		expect(mockState.getFolderFileMediaMetadata).not.toHaveBeenCalled();
	});

	it("plays shared folder music directly from the folder listing", async () => {
		mockState.getInfo.mockResolvedValueOnce({
			has_password: false,
			name: "Shared Root",
			shared_by: {
				avatar: {
					source: "gravatar",
					url_512: "https://www.gravatar.com/avatar/hash?s=512",
					url_1024: "https://www.gravatar.com/avatar/hash?s=1024",
					version: 2,
				},
				name: "Alice Example",
			},
			share_type: "folder",
		} as never);
		mockState.listContent.mockResolvedValueOnce({
			files: [
				{ id: 2, mime_type: "audio/mpeg", name: "Song.mp3", size: 2 },
				{ id: 3, mime_type: "text/plain", name: "notes.txt", size: 3 },
			],
			folders: [],
			next_file_cursor: null,
		} as never);

		render(<ShareViewPage />);

		expect(await screen.findByText("Song.mp3")).toBeInTheDocument();
		fireEvent.click(screen.getByRole("button", { name: "preview:Song.mp3" }));

		await waitFor(() => {
			expect(mockState.musicPlayTracks).toHaveBeenCalledWith(
				[
					expect.objectContaining({
						id: "share:share-token:file:2",
						name: "Song.mp3",
					}),
				],
				"share:share-token:file:2",
			);
		});
		expect(screen.queryByTestId("file-preview")).not.toBeInTheDocument();
	});
});
