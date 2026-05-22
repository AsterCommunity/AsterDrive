import type {
	ArchiveFilenameEncoding,
	ArchivePreviewManifest,
} from "@/types/api";

export type ArchivePreviewLoadManifest = (options?: {
	signal?: AbortSignal;
	filenameEncoding?: ArchiveFilenameEncoding;
}) => Promise<ArchivePreviewManifest>;

export interface ArchivePreviewProps {
	loadManifest?: ArchivePreviewLoadManifest;
}

export type ArchiveEntry = ArchivePreviewManifest["entries"][number];
export type ArchivePreviewErrorKind =
	| "disabled"
	| "invalid"
	| "rejected"
	| "sourceTooLarge"
	| "unsupported"
	| "generic";
export type ArchiveDirectoryEntry = {
	path: string;
	name: string;
	parent: string | null;
	kind: "directory";
	size: 0;
	compressed_size: 0;
	modified_at: null;
	synthetic: boolean;
};
export type ArchiveBrowserEntry = ArchiveEntry | ArchiveDirectoryEntry;
export type ArchiveBreadcrumbItem = {
	path: string | null;
	name: string;
};
export type ArchivePreviewState = {
	manifest: ArchivePreviewManifest | null;
	query: string;
	currentFolder: string | null;
	loading: boolean;
	pending: boolean;
	error: ArchivePreviewErrorKind | null;
	reloadKey: number;
	filenameEncoding: ArchiveFilenameEncoding;
};
export type ArchivePreviewAction =
	| { type: "manifestLoaderUnavailable" }
	| { type: "loadStarted" }
	| { type: "loadSucceeded"; manifest: ArchivePreviewManifest }
	| { type: "loadPending" }
	| { type: "loadFailed"; error: ArchivePreviewErrorKind }
	| { type: "retryRequested" }
	| { type: "queryChanged"; query: string }
	| { type: "currentFolderChanged"; currentFolder: string | null }
	| { type: "directoryOpened"; path: string }
	| {
			type: "filenameEncodingChanged";
			filenameEncoding: ArchiveFilenameEncoding;
	  };
