import { useEffect, useReducer } from "react";
import { supportsMediaData } from "@/lib/mediaDataSupport";
import { fileService } from "@/services/fileService";
import { ApiPendingError } from "@/services/http";
import { useMediaDataSupportStore } from "@/stores/mediaDataSupportStore";
import type {
	FileInfo,
	FileListItem,
	FolderInfo,
	FolderListItem,
	MediaMetadataInfo,
} from "@/types/api";
import { hasFileDetails, hasFolderDetails } from "./fileInfoDialogUtils";
import { mediaMetadataKindForFile } from "./mediaMetadataRows";

const MEDIA_METADATA_PENDING_MAX_RETRIES = 12;
const MEDIA_METADATA_PENDING_MAX_RETRY_DELAY_MS = 30_000;

interface ChildCount {
	folders: number;
	files: number;
}

interface FileInfoDataState {
	childCount: ChildCount | null;
	fileDetailsLoading: boolean;
	folderDetailsLoading: boolean;
	mediaMetadata: MediaMetadataInfo | null;
	mediaMetadataLoading: boolean;
	resolvedFile: FileInfo | null;
	resolvedFolder: FolderInfo | null;
}

type FileInfoDataAction =
	| { type: "resetFileDetails" }
	| { type: "useFileDetails"; file: FileInfo }
	| { type: "loadFileDetails" }
	| { type: "fileDetailsLoaded"; file: FileInfo | null }
	| { type: "resetFolderDetails" }
	| { type: "useFolderDetails"; folder: FolderInfo }
	| { type: "loadFolderDetails" }
	| { type: "folderDetailsLoaded"; folder: FolderInfo | null }
	| { type: "resetChildCount" }
	| { type: "childCountLoaded"; childCount: ChildCount | null }
	| { type: "resetMediaMetadata" }
	| { type: "mediaMetadataLoading" }
	| { type: "mediaMetadataLoaded"; metadata: MediaMetadataInfo | null };

const initialDataState: FileInfoDataState = {
	childCount: null,
	fileDetailsLoading: false,
	folderDetailsLoading: false,
	mediaMetadata: null,
	mediaMetadataLoading: false,
	resolvedFile: null,
	resolvedFolder: null,
};

function fileInfoDataReducer(
	state: FileInfoDataState,
	action: FileInfoDataAction,
): FileInfoDataState {
	switch (action.type) {
		case "resetFileDetails":
			return {
				...state,
				fileDetailsLoading: false,
				resolvedFile: null,
			};
		case "useFileDetails":
			return {
				...state,
				fileDetailsLoading: false,
				resolvedFile: action.file,
			};
		case "loadFileDetails":
			return {
				...state,
				fileDetailsLoading: true,
				resolvedFile: null,
			};
		case "fileDetailsLoaded":
			return {
				...state,
				fileDetailsLoading: false,
				resolvedFile: action.file,
			};
		case "resetFolderDetails":
			return {
				...state,
				folderDetailsLoading: false,
				resolvedFolder: null,
			};
		case "useFolderDetails":
			return {
				...state,
				folderDetailsLoading: false,
				resolvedFolder: action.folder,
			};
		case "loadFolderDetails":
			return {
				...state,
				folderDetailsLoading: true,
				resolvedFolder: null,
			};
		case "folderDetailsLoaded":
			return {
				...state,
				folderDetailsLoading: false,
				resolvedFolder: action.folder,
			};
		case "resetChildCount":
			return {
				...state,
				childCount: null,
			};
		case "childCountLoaded":
			return {
				...state,
				childCount: action.childCount,
			};
		case "resetMediaMetadata":
			return {
				...state,
				mediaMetadata: null,
				mediaMetadataLoading: false,
			};
		case "mediaMetadataLoading":
			return {
				...state,
				mediaMetadataLoading: true,
			};
		case "mediaMetadataLoaded":
			return {
				...state,
				mediaMetadata: action.metadata,
				mediaMetadataLoading: false,
			};
	}
}

function mediaMetadataPendingRetryDelay(error: unknown) {
	if (!(error instanceof ApiPendingError)) {
		return null;
	}

	const retryAfterSeconds = Number.isFinite(error.retryAfterSeconds)
		? error.retryAfterSeconds
		: 2;
	return Math.min(
		MEDIA_METADATA_PENDING_MAX_RETRY_DELAY_MS,
		Math.max(1, retryAfterSeconds) * 1000,
	);
}

export function useFileInfoDialogData({
	open,
	renderedFile,
	renderedFolder,
}: {
	open: boolean;
	renderedFile?: FileInfo | FileListItem;
	renderedFolder?: FolderInfo | FolderListItem;
}) {
	const [state, dispatch] = useReducer(fileInfoDataReducer, initialDataState);
	const mediaDataSupport = useMediaDataSupportStore((store) => store.config);
	const mediaDataSupportLoaded = useMediaDataSupportStore(
		(store) => store.isLoaded,
	);
	const loadMediaDataSupport = useMediaDataSupportStore((store) => store.load);
	const renderedMediaMetadataKind = renderedFile
		? mediaMetadataKindForFile(renderedFile)
		: null;
	const canRequestMediaMetadata =
		mediaDataSupportLoaded &&
		renderedFile != null &&
		supportsMediaData(renderedFile, mediaDataSupport);

	useEffect(() => {
		if (!mediaDataSupportLoaded) {
			void loadMediaDataSupport();
		}
	}, [loadMediaDataSupport, mediaDataSupportLoaded]);

	useEffect(() => {
		if (!open || !renderedFile) {
			dispatch({ type: "resetFileDetails" });
			return;
		}
		if (hasFileDetails(renderedFile)) {
			dispatch({ type: "useFileDetails", file: renderedFile });
			return;
		}

		let cancelled = false;
		dispatch({ type: "loadFileDetails" });
		fileService
			.getFile(renderedFile.id)
			.then((data) => {
				if (!cancelled) {
					dispatch({ type: "fileDetailsLoaded", file: data });
				}
			})
			.catch(() => {
				if (!cancelled) {
					dispatch({ type: "fileDetailsLoaded", file: null });
				}
			});

		return () => {
			cancelled = true;
		};
	}, [renderedFile, open]);

	useEffect(() => {
		if (!open || !renderedFolder) {
			dispatch({ type: "resetFolderDetails" });
			return;
		}
		if (hasFolderDetails(renderedFolder)) {
			dispatch({ type: "useFolderDetails", folder: renderedFolder });
			return;
		}

		let cancelled = false;
		dispatch({ type: "loadFolderDetails" });
		fileService
			.getFolderInfo(renderedFolder.id)
			.then((data) => {
				if (!cancelled) {
					dispatch({ type: "folderDetailsLoaded", folder: data });
				}
			})
			.catch(() => {
				if (!cancelled) {
					dispatch({ type: "folderDetailsLoaded", folder: null });
				}
			});

		return () => {
			cancelled = true;
		};
	}, [renderedFolder, open]);

	useEffect(() => {
		if (!open || !renderedFolder) {
			dispatch({ type: "resetChildCount" });
			return;
		}

		let cancelled = false;
		dispatch({ type: "resetChildCount" });
		fileService
			.listFolder(renderedFolder.id, { folder_limit: 0, file_limit: 0 })
			.then((res) => {
				if (!cancelled) {
					dispatch({
						type: "childCountLoaded",
						childCount: {
							folders: res.folders_total,
							files: res.files_total,
						},
					});
				}
			})
			.catch(() => {
				if (!cancelled) {
					dispatch({ type: "childCountLoaded", childCount: null });
				}
			});

		return () => {
			cancelled = true;
		};
	}, [open, renderedFolder]);

	useEffect(() => {
		if (
			!open ||
			!renderedFile ||
			!renderedMediaMetadataKind ||
			!canRequestMediaMetadata
		) {
			dispatch({ type: "resetMediaMetadata" });
			return;
		}

		const controller = new AbortController();
		let retryTimer: number | null = null;
		let cancelled = false;

		const loadMetadata = (attempt: number) => {
			dispatch({ type: "mediaMetadataLoading" });
			fileService
				.getMediaMetadata(renderedFile.id, { signal: controller.signal })
				.then((metadata) => {
					if (cancelled || controller.signal.aborted) return;
					dispatch({ type: "mediaMetadataLoaded", metadata });
				})
				.catch((error) => {
					if (cancelled || controller.signal.aborted) return;
					const retryDelayMs = mediaMetadataPendingRetryDelay(error);
					if (
						retryDelayMs !== null &&
						attempt < MEDIA_METADATA_PENDING_MAX_RETRIES
					) {
						retryTimer = window.setTimeout(() => {
							retryTimer = null;
							loadMetadata(attempt + 1);
						}, retryDelayMs);
						return;
					}
					dispatch({ type: "mediaMetadataLoaded", metadata: null });
				});
		};

		dispatch({ type: "resetMediaMetadata" });
		loadMetadata(0);

		return () => {
			cancelled = true;
			controller.abort();
			if (retryTimer !== null) {
				window.clearTimeout(retryTimer);
			}
		};
	}, [open, renderedFile, renderedMediaMetadataKind, canRequestMediaMetadata]);

	return {
		...state,
		canRequestMediaMetadata,
		renderedMediaMetadataKind,
	};
}
