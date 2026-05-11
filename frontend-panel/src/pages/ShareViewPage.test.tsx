import { fireEvent, render, screen, waitFor } from "@testing-library/react";
import { beforeEach, describe, expect, it, vi } from "vitest";
import { FOLDER_LIMIT } from "@/lib/constants";
import ShareViewPage from "@/pages/ShareViewPage";

const mockState = vi.hoisted(() => ({
	downloadFolderFileUrl: vi.fn(
		(token: string, fileId: number) =>
			`https://download/${token}/files/${fileId}`,
	),
	downloadFolderPath: vi.fn(
		(token: string, fileId: number) => `/s/${token}/files/${fileId}/download`,
	),
	downloadPath: vi.fn((token: string) => `/s/${token}/download`),
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
	thumbnailPath: vi.fn((token: string) => `/s/${token}/thumbnail`),
	downloadUrl: vi.fn((token: string) => `https://download/${token}`),
	getInfo: vi.fn(),
	handleApiError: vi.fn(),
	listContent: vi.fn(),
	listSubfolderContent: vi.fn(),
	openWindow: vi.fn(),
	params: { token: "share-token" as string | undefined },
	previewAppStore: {
		isLoaded: false,
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

vi.mock("react-i18next", () => ({
	useTranslation: () => ({
		t: mockState.translate,
	}),
}));

vi.mock("react-router-dom", () => ({
	useParams: () => mockState.params,
}));

vi.mock("sonner", () => ({
	toast: {
		success: (...args: unknown[]) => mockState.toastSuccess(...args),
	},
}));

vi.mock("@/stores/previewAppStore", () => ({
	usePreviewAppStore: (
		selector: (state: typeof mockState.previewAppStore) => unknown,
	) => selector(mockState.previewAppStore),
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
		downloadPath,
		editable,
		videoStreamLinkFactory,
	}: {
		file: { name: string };
		open?: boolean;
		downloadPath?: string;
		editable?: boolean;
		videoStreamLinkFactory?: () => Promise<unknown>;
	}) =>
		open ? (
			<div
				data-testid="file-preview"
				data-name={file.name}
				data-download-path={downloadPath ?? ""}
				data-editable={String(Boolean(editable))}
				data-has-video-stream-link-factory={String(
					Boolean(videoStreamLinkFactory),
				)}
			/>
		) : null,
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

vi.mock("@/components/files/ReadOnlyFileCollection", () => ({
	ReadOnlyFileCollection: ({
		folders,
		files,
		onFileClick,
		onFileDownload,
		onFolderClick,
		emptyTitle,
		emptyDescription,
	}: {
		folders: Array<{ id: number; name: string }>;
		files: Array<{ id: number; name: string; mime_type: string; size: number }>;
		onFileClick: (file: {
			id: number;
			name: string;
			mime_type: string;
			size: number;
		}) => void;
		onFileDownload: (file: { id: number; name: string }) => void;
		onFolderClick: (folder: { id: number; name: string }) => void;
		emptyTitle: string;
		emptyDescription: string;
	}) => (
		<div>
			{folders.length === 0 && files.length === 0 ? (
				<div>{`${emptyTitle}:${emptyDescription}`}</div>
			) : null}
			{folders.map((folder) => (
				<button
					key={folder.id}
					type="button"
					onClick={() => onFolderClick(folder)}
				>
					{`folder:${folder.name}`}
				</button>
			))}
			{files.map((file) => (
				<div key={file.id}>
					<span>{file.name}</span>
					<button type="button" onClick={() => onFileClick(file)}>
						{`preview:${file.name}`}
					</button>
					<button type="button" onClick={() => onFileDownload(file)}>
						{`download:${file.name}`}
					</button>
				</div>
			))}
		</div>
	),
}));

vi.mock("@/components/layout/ShareTopBar", () => ({
	ShareTopBar: () => <div>share-top-bar</div>,
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
		thumbnailPath: (...args: unknown[]) => mockState.thumbnailPath(...args),
		downloadUrl: (...args: unknown[]) => mockState.downloadUrl(...args),
		getInfo: (...args: unknown[]) => mockState.getInfo(...args),
		listContent: (...args: unknown[]) => mockState.listContent(...args),
		listSubfolderContent: (...args: unknown[]) =>
			mockState.listSubfolderContent(...args),
		verifyPassword: (...args: unknown[]) => mockState.verifyPassword(...args),
	},
}));

describe("ShareViewPage", () => {
	beforeEach(() => {
		mockState.downloadFolderFileUrl.mockClear();
		mockState.downloadFolderPath.mockClear();
		mockState.downloadPath.mockClear();
		mockState.createStreamSession.mockClear();
		mockState.createFolderFileStreamSession.mockClear();
		mockState.thumbnailPath.mockClear();
		mockState.downloadUrl.mockClear();
		mockState.getInfo.mockReset();
		mockState.handleApiError.mockReset();
		mockState.listContent.mockReset();
		mockState.listSubfolderContent.mockReset();
		mockState.openWindow.mockReset();
		mockState.params = { token: "share-token" };
		mockState.previewAppStore.load.mockReset();
		mockState.previewAppStore.isLoaded = false;
		mockState.previewAppStore.load.mockResolvedValue(undefined);
		mockState.toastSuccess.mockReset();
		mockState.verifyPassword.mockReset();
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
		mockState.listContent.mockResolvedValueOnce({
			files: [],
			folders: [{ id: 1, name: "Docs" }],
			next_file_cursor: null,
		} as never);

		render(<ShareViewPage />);

		await screen.findByText("Secret Folder");
		expect(mockState.previewAppStore.load).toHaveBeenCalledTimes(1);
		fireEvent.change(screen.getByPlaceholderText("password"), {
			target: { value: "letmein" },
		});
		fireEvent.click(screen.getByRole("button", { name: "verify" }));

		await waitFor(() => {
			expect(mockState.verifyPassword).toHaveBeenCalledWith(
				"share-token",
				"letmein",
			);
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
		expect(screen.getByTestId("file-preview")).toHaveAttribute(
			"data-editable",
			"false",
		);
		expect(screen.getByTestId("file-preview")).toHaveAttribute(
			"data-has-video-stream-link-factory",
			"true",
		);

		fireEvent.click(screen.getByRole("button", { name: /files:download/i }));

		expect(mockState.downloadUrl).toHaveBeenCalledWith("share-token");
		expect(mockState.openWindow).toHaveBeenCalledWith(
			"https://download/share-token",
			"_blank",
		);
	});

	it("navigates folder shares and uses the folder-specific preview and download paths", async () => {
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
			files: [{ id: 2, mime_type: "text/plain", name: "root.txt", size: 2 }],
			folders: [{ id: 1, name: "Docs" }],
			next_file_cursor: null,
		} as never);
		mockState.listSubfolderContent.mockResolvedValueOnce({
			files: [{ id: 5, mime_type: "text/plain", name: "nested.txt", size: 5 }],
			folders: [],
			next_file_cursor: null,
		} as never);

		render(<ShareViewPage />);

		expect(await screen.findByText("Shared Root")).toBeInTheDocument();
		expect(screen.getByText("shared-by:Alice Example")).toBeInTheDocument();
		expect(screen.queryByText("share-content")).not.toBeInTheDocument();
		expect(
			screen.getByText(
				"avatar:Alice Example:https://www.gravatar.com/avatar/hash?s=512",
			),
		).toBeInTheDocument();
		fireEvent.click(await screen.findByRole("button", { name: "folder:Docs" }));

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
			screen.getByRole("button", { name: "Shared Root" }),
		).toBeInTheDocument();
		expect(screen.getByText("Docs")).toBeInTheDocument();

		fireEvent.click(screen.getByRole("button", { name: "preview:nested.txt" }));

		expect(await screen.findByTestId("file-preview")).toHaveAttribute(
			"data-download-path",
			"/s/share-token/files/5/download",
		);
		expect(screen.getByTestId("file-preview")).toHaveAttribute(
			"data-editable",
			"false",
		);
		expect(screen.getByTestId("file-preview")).toHaveAttribute(
			"data-has-video-stream-link-factory",
			"true",
		);

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
		);
	});
});
