import type { ResolveFileResourceHandle } from "@/hooks/useFileResource";
import type {
	ArchiveFilenameEncoding,
	ArchivePreviewManifest,
	PreviewLinkInfo,
	ShareStreamSessionInfo,
	WopiLaunchSession,
} from "@/types/api";

export interface FilePreviewResourcePaths {
	download: string;
	imagePreview?: string;
	thumbnail?: string;
}

export interface FilePreviewArchiveOptions {
	filenameEncoding?: ArchiveFilenameEncoding;
	signal?: AbortSignal;
}

export interface FilePreviewResourceActions {
	loadArchiveManifest?: (
		options?: FilePreviewArchiveOptions,
	) => Promise<ArchivePreviewManifest>;
	createMediaStreamSession?: () => Promise<ShareStreamSessionInfo>;
	createExternalPreviewLink?: () => Promise<PreviewLinkInfo>;
	launchWopiSession?: (appKey: string) => Promise<WopiLaunchSession>;
}

export interface FilePreviewResources {
	actions?: FilePreviewResourceActions;
	paths: FilePreviewResourcePaths;
	resolve: ResolveFileResourceHandle;
	scope: "personal" | "team" | "share";
}
