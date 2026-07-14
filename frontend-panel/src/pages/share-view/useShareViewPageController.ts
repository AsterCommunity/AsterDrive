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
import type { SortBy, SortOrder } from "@/stores/fileStore/types";
import { useMusicPlayerStore } from "@/stores/musicPlayerStore";
import { usePreviewAppStore } from "@/stores/previewAppStore";
import type {
	FileInfo,
	FileListItem,
	FolderAncestorItem,
	FolderContents,
	FolderListParams,
	SharePublicInfo,
} from "@/types/api";
import { ApiErrorCode } from "@/types/api-helpers";
import type { ShareBreadcrumbItem } from "./types";

const SHARE_PAGE_SIZE = 100;
const DEFAULT_SORT_BY: SortBy = "name";
const DEFAULT_SORT_ORDER: SortOrder = "asc";

function shareFolderListParams(
	sortBy: SortBy,
	sortOrder: SortOrder,
	overrides: FolderListParams = {},
): FolderListParams {
	return {
		folder_limit: FOLDER_LIMIT,
		file_limit: SHARE_PAGE_SIZE,
		...(sortBy === DEFAULT_SORT_BY ? {} : { sort_by: sortBy }),
		...(sortOrder === DEFAULT_SORT_ORDER ? {} : { sort_order: sortOrder }),
		...overrides,
	};
}

type FileCursor = NonNullable<FolderContents["next_file_cursor"]>;

function loadMoreCursorKey(
	token: string,
	folderId: number | null,
	cursor: FileCursor,
) {
	return `${token}:${folderId ?? "root"}:${cursor.value}:${cursor.id}`;
}

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
	sortBy: SortBy;
	sortOrder: SortOrder;
	viewMode: "grid" | "list";
}

type ShareViewAction =
	| { type: "loadStart" }
	| {
			type: "loadSuccess";
			info: SharePublicInfo;
			folderContents: FolderContents | null;
			breadcrumb: ShareBreadcrumbItem[];
			passwordVerified: boolean;
	  }
	| { type: "loadError"; error: string }
	| { type: "setPassword"; password: string }
	| {
			type: "passwordVerified";
			folderContents: FolderContents | null;
			breadcrumb: ShareBreadcrumbItem[];
	  }
	| { type: "navigateStart" }
	| {
			type: "navigateSuccess";
			folderContents: FolderContents;
			breadcrumb: ShareBreadcrumbItem[];
	  }
	| { type: "navigateEnd" }
	| { type: "loadMoreStart" }
	| { type: "loadMoreSuccess"; folderContents: FolderContents }
	| { type: "loadMoreEnd" }
	| {
			type: "sortSuccess";
			folderContents: FolderContents;
			sortBy: SortBy;
			sortOrder: SortOrder;
	  }
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
	sortBy: DEFAULT_SORT_BY,
	sortOrder: DEFAULT_SORT_ORDER,
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
				breadcrumb: action.breadcrumb,
				error: null,
				folderContents: action.folderContents,
				info: action.info,
				loading: false,
				loadingMore: false,
				needsPassword: action.info.has_password && !action.passwordVerified,
				password: "",
				passwordVerified: action.passwordVerified,
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
				breadcrumb: action.breadcrumb,
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
			return {
				...state,
				breadcrumb: action.breadcrumb,
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
		case "sortSuccess":
			return {
				...state,
				folderContents: action.folderContents,
				navigating: false,
				sortBy: action.sortBy,
				sortOrder: action.sortOrder,
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

interface SharedFolderLocation {
	breadcrumb: ShareBreadcrumbItem[];
	folderContents: FolderContents;
}

function breadcrumbForSharedFolder(
	rootName: string,
	folderId: number,
	ancestors: FolderAncestorItem[],
): ShareBreadcrumbItem[] {
	if (ancestors.at(-1)?.id !== folderId) {
		throw new Error("shared folder ancestor chain does not match the route");
	}
	return [
		{ id: null, name: rootName },
		...ancestors.map((ancestor) => ({
			id: ancestor.id,
			name: ancestor.name,
		})),
	];
}

async function loadSharedFolderLocation({
	token,
	rootName,
	folderId,
	params,
}: {
	token: string;
	rootName: string;
	folderId: number | null;
	params: FolderListParams;
}): Promise<SharedFolderLocation> {
	if (folderId === null) {
		return {
			breadcrumb: [{ id: null, name: rootName }],
			folderContents: await shareService.listContent(token, params),
		};
	}

	const [folderContents, ancestors] = await Promise.all([
		shareService.listSubfolderContent(token, folderId, params),
		shareService.getSubfolderAncestors(token, folderId),
	]);
	return {
		breadcrumb: breadcrumbForSharedFolder(rootName, folderId, ancestors),
		folderContents,
	};
}

function errorMessageForShareLoad(error: unknown, t: (key: string) => string) {
	if (error instanceof ApiError) {
		if (error.code === ApiErrorCode.ShareExpired) {
			return t("errors:share_expired");
		}
		if (error.code === ApiErrorCode.ShareNotFound) {
			return t("errors:share_not_found");
		}
		if (error.code === ApiErrorCode.ShareDownloadLimitReached) {
			return t("share:download_limit_reached");
		}
		return error.message;
	}
	return t("share:failed_to_load_share");
}

function isSharePasswordRequired(error: unknown) {
	return (
		error instanceof ApiError &&
		error.code === ApiErrorCode.SharePasswordRequired
	);
}

export function useShareViewPageController({
	token,
	requestedFolderId = null,
	enabled = true,
	t,
}: {
	token?: string;
	requestedFolderId?: number | null;
	enabled?: boolean;
	t: (key: string) => string;
}) {
	const previewAppsLoaded = usePreviewAppStore((state) => state.isLoaded);
	const loadPreviewApps = usePreviewAppStore((state) => state.load);
	const playTracks = useMusicPlayerStore((state) => state.playTracks);
	const [state, dispatch] = useReducer(shareViewReducer, initialShareViewState);
	const sentinelRef = useRef<HTMLDivElement | null>(null);
	const loadingMoreCursorKeyRef = useRef<string | null>(null);
	const locationRequestIdRef = useRef(0);
	const sortRef = useRef({ sortBy: state.sortBy, sortOrder: state.sortOrder });
	sortRef.current = { sortBy: state.sortBy, sortOrder: state.sortOrder };
	const currentFolderId = requestedFolderId;
	const nextFileCursor = state.folderContents?.next_file_cursor ?? null;
	const nextFileCursorKey =
		token && nextFileCursor
			? loadMoreCursorKey(token, currentFolderId, nextFileCursor)
			: null;
	const hasMoreFiles = state.folderContents?.next_file_cursor != null;

	useEffect(() => {
		if (
			!nextFileCursorKey ||
			loadingMoreCursorKeyRef.current !== nextFileCursorKey
		) {
			loadingMoreCursorKeyRef.current = null;
		}
	}, [nextFileCursorKey]);

	const loadInfo = useCallback(async () => {
		if (!enabled || !token) return;
		const requestId = ++locationRequestIdRef.current;
		loadingMoreCursorKeyRef.current = null;
		dispatch({ type: "loadStart" });
		try {
			const data = await shareService.getInfo(token);
			if (data.share_type !== "folder" && requestedFolderId !== null) {
				if (requestId === locationRequestIdRef.current) {
					dispatch({ type: "loadError", error: t("errors:folder_not_found") });
				}
				return;
			}
			let location: SharedFolderLocation | null = null;
			let passwordVerified = !data.has_password;
			if (data.share_type === "folder") {
				try {
					location = await loadSharedFolderLocation({
						token,
						rootName: data.name,
						folderId: requestedFolderId,
						params: shareFolderListParams(
							sortRef.current.sortBy,
							sortRef.current.sortOrder,
						),
					});
					passwordVerified = true;
				} catch (error) {
					if (!data.has_password || !isSharePasswordRequired(error)) {
						throw error;
					}
				}
			}
			if (requestId !== locationRequestIdRef.current) return;
			dispatch({
				type: "loadSuccess",
				info: data,
				folderContents: location?.folderContents ?? null,
				breadcrumb: location?.breadcrumb ?? [],
				passwordVerified,
			});
		} catch (error) {
			if (requestId !== locationRequestIdRef.current) return;
			dispatch({
				type: "loadError",
				error: errorMessageForShareLoad(error, t),
			});
		}
	}, [enabled, requestedFolderId, token, t]);

	useEffect(() => {
		void loadInfo().catch(() => {});
	}, [loadInfo]);

	useEffect(() => {
		if (previewAppsLoaded) return;
		void loadPreviewApps();
	}, [loadPreviewApps, previewAppsLoaded]);

	const refreshFolder = useCallback(async () => {
		if (!token || !state.info || state.info.share_type !== "folder") return;
		const requestId = ++locationRequestIdRef.current;
		dispatch({ type: "navigateStart" });
		try {
			const location = await loadSharedFolderLocation({
				token,
				rootName: state.info.name,
				folderId: currentFolderId,
				params: shareFolderListParams(state.sortBy, state.sortOrder),
			});
			if (requestId !== locationRequestIdRef.current) return;
			dispatch({
				type: "navigateSuccess",
				folderContents: location.folderContents,
				breadcrumb: location.breadcrumb,
			});
		} catch (error) {
			if (requestId !== locationRequestIdRef.current) return;
			handleApiError(error);
			dispatch({ type: "navigateEnd" });
		}
	}, [currentFolderId, state.info, state.sortBy, state.sortOrder, token]);

	const updateSort = useCallback(
		async (sortBy: SortBy, sortOrder: SortOrder) => {
			if (
				!token ||
				(sortBy === state.sortBy && sortOrder === state.sortOrder)
			) {
				return;
			}

			const requestId = ++locationRequestIdRef.current;
			dispatch({ type: "navigateStart" });
			try {
				const params = shareFolderListParams(sortBy, sortOrder);
				const contents =
					currentFolderId === null
						? await shareService.listContent(token, params)
						: await shareService.listSubfolderContent(
								token,
								currentFolderId,
								params,
							);
				if (requestId !== locationRequestIdRef.current) return;
				dispatch({
					type: "sortSuccess",
					folderContents: contents,
					sortBy,
					sortOrder,
				});
			} catch (error) {
				if (requestId !== locationRequestIdRef.current) return;
				handleApiError(error);
				dispatch({ type: "navigateEnd" });
			}
		},
		[currentFolderId, state.sortBy, state.sortOrder, token],
	);

	const loadMoreShareFiles = useCallback(async () => {
		if (!token || state.loadingMore || !nextFileCursor || !nextFileCursorKey) {
			return;
		}
		if (loadingMoreCursorKeyRef.current === nextFileCursorKey) return;
		loadingMoreCursorKeyRef.current = nextFileCursorKey;
		dispatch({ type: "loadMoreStart" });
		try {
			const contents =
				currentFolderId === null
					? await shareService.listContent(
							token,
							shareFolderListParams(state.sortBy, state.sortOrder, {
								folder_limit: 0,
								file_limit: SHARE_PAGE_SIZE,
								file_after_value: nextFileCursor.value,
								file_after_id: nextFileCursor.id,
							}),
						)
					: await shareService.listSubfolderContent(token, currentFolderId, {
							...shareFolderListParams(state.sortBy, state.sortOrder),
							folder_limit: 0,
							file_limit: SHARE_PAGE_SIZE,
							file_after_value: nextFileCursor.value,
							file_after_id: nextFileCursor.id,
						});
			if (loadingMoreCursorKeyRef.current !== nextFileCursorKey) return;
			dispatch({ type: "loadMoreSuccess", folderContents: contents });
		} catch (error) {
			if (loadingMoreCursorKeyRef.current === nextFileCursorKey) {
				loadingMoreCursorKeyRef.current = null;
			}
			handleApiError(error);
			dispatch({ type: "loadMoreEnd" });
		}
	}, [
		currentFolderId,
		nextFileCursor,
		nextFileCursorKey,
		state.loadingMore,
		state.sortBy,
		state.sortOrder,
		token,
	]);

	useEffect(() => {
		if (!hasMoreFiles || state.loadingMore || !nextFileCursorKey) return;
		const el = sentinelRef.current;
		if (!el) return;
		const observer = new IntersectionObserver(
			(entries) => {
				if (
					entries[0].isIntersecting &&
					loadingMoreCursorKeyRef.current !== nextFileCursorKey
				) {
					void loadMoreShareFiles().catch(() => {});
				}
			},
			{ rootMargin: "200px" },
		);
		observer.observe(el);
		return () => observer.disconnect();
	}, [hasMoreFiles, state.loadingMore, nextFileCursorKey, loadMoreShareFiles]);

	const handleVerifyPassword = useCallback(
		async (event: FormEvent) => {
			event.preventDefault();
			if (!token) return;
			const requestId = ++locationRequestIdRef.current;
			try {
				await shareService.verifyPassword(token, { password: state.password });
			} catch (error) {
				if (requestId !== locationRequestIdRef.current) return;
				handleApiError(error);
				return;
			}

			toast.success(t("share:password_verified"));
			try {
				const location =
					state.info?.share_type === "folder"
						? await loadSharedFolderLocation({
								token,
								rootName: state.info.name,
								folderId: currentFolderId,
								params: shareFolderListParams(state.sortBy, state.sortOrder),
							})
						: null;
				if (requestId !== locationRequestIdRef.current) return;
				dispatch({
					type: "passwordVerified",
					folderContents: location?.folderContents ?? null,
					breadcrumb: location?.breadcrumb ?? [],
				});
			} catch (error) {
				if (requestId !== locationRequestIdRef.current) return;
				dispatch({
					type: "loadError",
					error: errorMessageForShareLoad(error, t),
				});
			}
		},
		[
			currentFolderId,
			state.info,
			state.password,
			state.sortBy,
			state.sortOrder,
			token,
			t,
		],
	);

	const handleDownload = useCallback(() => {
		if (!token) return;
		const url = shareService.downloadUrl(token);
		window.open(url, "_blank", "noopener,noreferrer");
	}, [token]);

	const handleFolderFileDownload = useCallback(
		(file: FileListItem) => {
			if (!token) return;
			const url = shareService.downloadFolderFileUrl(token, file.id);
			window.open(url, "_blank", "noopener,noreferrer");
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
		refreshFolder,
		setSortBy: (sortBy: SortBy) => {
			void updateSort(sortBy, state.sortOrder);
		},
		setSortOrder: (sortOrder: SortOrder) => {
			void updateSort(state.sortBy, sortOrder);
		},
		setPassword: (password: string) =>
			dispatch({ type: "setPassword", password }),
		setPreviewFile: (file: FileInfo | FileListItem | null) =>
			dispatch({ type: "setPreviewFile", file }),
		setViewMode: (viewMode: "grid" | "list") =>
			dispatch({ type: "setViewMode", viewMode }),
	};
}
