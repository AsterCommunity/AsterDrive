import type { FormEvent } from "react";
import { useCallback, useEffect, useReducer, useRef } from "react";
import { toast } from "sonner";
import { handleApiError } from "@/hooks/useApiError";
import { FOLDER_LIMIT } from "@/lib/constants";
import {
	buildShareFolderMusicQueue,
	buildSingleShareMusicTrack,
	hydrateMusicQueueForPlayback,
	isMusicFile,
} from "@/lib/musicPlayer";
import { ApiError } from "@/services/http";
import { shareService } from "@/services/shareService";
import { useMusicPlayerStore } from "@/stores/musicPlayerStore";
import { usePreviewAppStore } from "@/stores/previewAppStore";
import type {
	FileInfo,
	FileListItem,
	FolderContents,
	SharePublicInfo,
} from "@/types/api";
import { ErrorCode } from "@/types/api-helpers";
import type { ShareBreadcrumbItem } from "./types";

const SHARE_PAGE_SIZE = 100;
const sharePageParams = {
	folder_limit: FOLDER_LIMIT,
	file_limit: SHARE_PAGE_SIZE,
};

interface ShareViewState {
	breadcrumb: ShareBreadcrumbItem[];
	error: string | null;
	folderContents: FolderContents | null;
	info: SharePublicInfo | null;
	loading: boolean;
	loadingMore: boolean;
	navigating: boolean;
	needsPassword: boolean;
	password: string;
	passwordVerified: boolean;
	previewFile: FileInfo | FileListItem | null;
	viewMode: "grid" | "list";
}

type ShareViewAction =
	| { type: "loadStart" }
	| {
			type: "loadSuccess";
			info: SharePublicInfo;
			folderContents: FolderContents | null;
	  }
	| { type: "loadError"; error: string }
	| { type: "setPassword"; password: string }
	| { type: "passwordVerified"; folderContents: FolderContents | null }
	| { type: "navigateStart" }
	| {
			type: "navigateSuccess";
			folderContents: FolderContents;
			folderId: number | null;
			folderName?: string;
	  }
	| { type: "navigateEnd" }
	| { type: "loadMoreStart" }
	| { type: "loadMoreSuccess"; folderContents: FolderContents }
	| { type: "loadMoreEnd" }
	| { type: "setPreviewFile"; file: FileInfo | FileListItem | null }
	| { type: "setViewMode"; viewMode: "grid" | "list" };

const initialShareViewState: ShareViewState = {
	breadcrumb: [],
	error: null,
	folderContents: null,
	info: null,
	loading: true,
	loadingMore: false,
	navigating: false,
	needsPassword: false,
	password: "",
	passwordVerified: false,
	previewFile: null,
	viewMode: "grid",
};

function shareViewReducer(
	state: ShareViewState,
	action: ShareViewAction,
): ShareViewState {
	switch (action.type) {
		case "loadStart":
			return {
				...state,
				error: null,
				loading: true,
			};
		case "loadSuccess":
			return {
				...state,
				breadcrumb:
					action.info.share_type === "folder" && !action.info.has_password
						? [{ id: null, name: action.info.name }]
						: state.breadcrumb,
				error: null,
				folderContents: action.folderContents,
				info: action.info,
				loading: false,
				needsPassword: action.info.has_password,
			};
		case "loadError":
			return {
				...state,
				error: action.error,
				loading: false,
			};
		case "setPassword":
			return {
				...state,
				password: action.password,
			};
		case "passwordVerified":
			return {
				...state,
				breadcrumb:
					state.info?.share_type === "folder"
						? [{ id: null, name: state.info.name }]
						: state.breadcrumb,
				folderContents: action.folderContents ?? state.folderContents,
				needsPassword: false,
				passwordVerified: true,
			};
		case "navigateStart":
			return {
				...state,
				navigating: true,
			};
		case "navigateSuccess": {
			const breadcrumb =
				action.folderId === null
					? state.breadcrumb.slice(0, 1)
					: updateBreadcrumb(
							state.breadcrumb,
							action.folderId,
							action.folderName,
						);
			return {
				...state,
				breadcrumb,
				folderContents: action.folderContents,
				navigating: false,
			};
		}
		case "navigateEnd":
			return {
				...state,
				navigating: false,
			};
		case "loadMoreStart":
			return {
				...state,
				loadingMore: true,
			};
		case "loadMoreSuccess":
			return {
				...state,
				folderContents: state.folderContents
					? {
							...state.folderContents,
							files: [
								...state.folderContents.files,
								...action.folderContents.files,
							],
							next_file_cursor: action.folderContents.next_file_cursor,
						}
					: state.folderContents,
				loadingMore: false,
			};
		case "loadMoreEnd":
			return {
				...state,
				loadingMore: false,
			};
		case "setPreviewFile":
			return {
				...state,
				previewFile: action.file,
			};
		case "setViewMode":
			return {
				...state,
				viewMode: action.viewMode,
			};
	}
}

function updateBreadcrumb(
	breadcrumb: ShareBreadcrumbItem[],
	folderId: number,
	folderName?: string,
) {
	const existingIndex = breadcrumb.findIndex((item) => item.id === folderId);
	if (existingIndex >= 0) {
		return breadcrumb.slice(0, existingIndex + 1);
	}
	return [...breadcrumb, { id: folderId, name: folderName ?? "" }];
}

function errorMessageForShareLoad(error: unknown, t: (key: string) => string) {
	if (error instanceof ApiError) {
		if (error.code === ErrorCode.ShareExpired) {
			return t("errors:share_expired");
		}
		if (error.code === ErrorCode.ShareNotFound) {
			return t("errors:share_not_found");
		}
		if (error.code === ErrorCode.ShareDownloadLimitReached) {
			return t("share:download_limit_reached");
		}
		return error.message;
	}
	return t("share:failed_to_load_share");
}

export function useShareViewPageController({
	token,
	t,
}: {
	token?: string;
	t: (key: string) => string;
}) {
	const previewAppsLoaded = usePreviewAppStore((state) => state.isLoaded);
	const loadPreviewApps = usePreviewAppStore((state) => state.load);
	const playTracks = useMusicPlayerStore((state) => state.playTracks);
	const [state, dispatch] = useReducer(shareViewReducer, initialShareViewState);
	const sentinelRef = useRef<HTMLDivElement | null>(null);
	const hasMoreFiles = state.folderContents?.next_file_cursor != null;

	const loadInfo = useCallback(async () => {
		if (!token) return;
		dispatch({ type: "loadStart" });
		try {
			const data = await shareService.getInfo(token);
			const folderContents =
				data.share_type === "folder" && !data.has_password
					? await shareService.listContent(token, sharePageParams)
					: null;
			dispatch({
				type: "loadSuccess",
				info: data,
				folderContents,
			});
		} catch (error) {
			dispatch({
				type: "loadError",
				error: errorMessageForShareLoad(error, t),
			});
		}
	}, [token, t]);

	useEffect(() => {
		void loadInfo().catch(() => {});
	}, [loadInfo]);

	useEffect(() => {
		if (previewAppsLoaded) return;
		void loadPreviewApps();
	}, [loadPreviewApps, previewAppsLoaded]);

	const navigateToFolder = useCallback(
		async (folderId: number | null, folderName?: string) => {
			if (!token) return;
			dispatch({ type: "navigateStart" });
			try {
				const contents =
					folderId === null
						? await shareService.listContent(token, sharePageParams)
						: await shareService.listSubfolderContent(
								token,
								folderId,
								sharePageParams,
							);
				dispatch({
					type: "navigateSuccess",
					folderContents: contents,
					folderId,
					folderName,
				});
			} catch (error) {
				handleApiError(error);
				dispatch({ type: "navigateEnd" });
			}
		},
		[token],
	);

	const loadMoreShareFiles = useCallback(async () => {
		if (
			!token ||
			!state.folderContents ||
			state.loadingMore ||
			!state.folderContents.next_file_cursor
		) {
			return;
		}
		dispatch({ type: "loadMoreStart" });
		try {
			const currentId =
				state.breadcrumb[state.breadcrumb.length - 1]?.id ?? null;
			const cursor = state.folderContents.next_file_cursor;
			const contents =
				currentId === null
					? await shareService.listContent(token, {
							folder_limit: 0,
							file_limit: SHARE_PAGE_SIZE,
							file_after_value: cursor.value,
							file_after_id: cursor.id,
						})
					: await shareService.listSubfolderContent(token, currentId, {
							folder_limit: 0,
							file_limit: SHARE_PAGE_SIZE,
							file_after_value: cursor.value,
							file_after_id: cursor.id,
						});
			dispatch({ type: "loadMoreSuccess", folderContents: contents });
		} catch (error) {
			handleApiError(error);
			dispatch({ type: "loadMoreEnd" });
		}
	}, [state.breadcrumb, state.folderContents, state.loadingMore, token]);

	useEffect(() => {
		if (!hasMoreFiles || state.loadingMore) return;
		const el = sentinelRef.current;
		if (!el) return;
		const observer = new IntersectionObserver(
			(entries) => {
				if (entries[0].isIntersecting)
					void loadMoreShareFiles().catch(() => {});
			},
			{ rootMargin: "200px" },
		);
		observer.observe(el);
		return () => observer.disconnect();
	}, [hasMoreFiles, state.loadingMore, loadMoreShareFiles]);

	const handleVerifyPassword = useCallback(
		async (event: FormEvent) => {
			event.preventDefault();
			if (!token) return;
			try {
				await shareService.verifyPassword(token, state.password);
				toast.success(t("share:password_verified"));
				const folderContents =
					state.info?.share_type === "folder"
						? await shareService.listContent(token, sharePageParams)
						: null;
				dispatch({ type: "passwordVerified", folderContents });
			} catch (error) {
				handleApiError(error);
			}
		},
		[state.info, state.password, token, t],
	);

	const handleDownload = useCallback(() => {
		if (!token) return;
		const url = shareService.downloadUrl(token);
		window.open(url, "_blank");
	}, [token]);

	const handleFolderFileDownload = useCallback(
		(file: FileListItem) => {
			if (!token) return;
			const url = shareService.downloadFolderFileUrl(token, file.id);
			window.open(url, "_blank");
		},
		[token],
	);

	const playSharedMusicFile = useCallback(
		(file: FileInfo | FileListItem) => {
			if (!token || !state.info || !isMusicFile(file)) return false;

			const queue =
				state.info.share_type === "file"
					? [buildSingleShareMusicTrack(state.info, token)].filter(
							(track): track is NonNullable<typeof track> => track !== null,
						)
					: buildShareFolderMusicQueue(
							token,
							state.folderContents?.files ?? [file],
						);
			const activeTrack = queue.find((track) =>
				state.info?.share_type === "file"
					? track.id === `share:${token}:file`
					: track.id === `share:${token}:file:${file.id}`,
			);
			if (!activeTrack) return false;

			void hydrateMusicQueueForPlayback(queue, activeTrack.id)
				.then((hydratedQueue) => {
					playTracks(hydratedQueue, activeTrack.id);
				})
				.catch((error) => {
					handleApiError(error);
					dispatch({ type: "setPreviewFile", file });
				});
			return true;
		},
		[playTracks, state.folderContents?.files, state.info, token],
	);

	const handlePreviewFile = useCallback(
		(file: FileInfo | FileListItem) => {
			if (playSharedMusicFile(file)) return;
			dispatch({ type: "setPreviewFile", file });
		},
		[playSharedMusicFile],
	);

	return {
		...state,
		hasMoreFiles,
		sentinelRef,
		handleDownload,
		handleFolderFileDownload,
		handlePreviewFile,
		handleVerifyPassword,
		navigateToFolder,
		setPassword: (password: string) =>
			dispatch({ type: "setPassword", password }),
		setPreviewFile: (file: FileInfo | FileListItem | null) =>
			dispatch({ type: "setPreviewFile", file }),
		setViewMode: (viewMode: "grid" | "list") =>
			dispatch({ type: "setViewMode", viewMode }),
	};
}
