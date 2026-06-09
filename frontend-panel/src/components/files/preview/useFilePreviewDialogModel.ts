import {
	useCallback,
	useEffect,
	useMemo,
	useReducer,
	useRef,
	useState,
} from "react";
import { supportsAudioMediaData } from "@/lib/mediaDataSupport";
import { fileService } from "@/services/fileService";
import { useMediaDataSupportStore } from "@/stores/mediaDataSupportStore";
import type { MusicPlayerTrack } from "@/stores/musicPlayerStore";
import { usePreviewAppStore } from "@/stores/previewAppStore";
import { useThumbnailSupportStore } from "@/stores/thumbnailSupportStore";
import type {
	ArchivePreviewManifest,
	FileInfo,
	FileListItem,
	PreviewLinkInfo,
	ShareStreamSessionInfo,
	WopiLaunchSession,
} from "@/types/api";
import {
	detectFilePreviewProfile,
	getFileExtension,
} from "./file-capabilities";
import { resolveOpenWithOptionLabel } from "./openWithLabel";
import type { OpenWithMode, OpenWithOption } from "./types";
import { getVideoBrowserOpenWithOption } from "./video-browser-config";
import {
	createWopiSessionResource,
	type WopiSessionResource,
} from "./wopiSessionResource";

const PREVIEW_DIALOG_OPEN_ANIMATION_MS = 120;
// Matches Tailwind's md breakpoint boundary.
const MOBILE_PREVIEW_MEDIA_QUERY = "(max-width: 767px)";

export interface FilePreviewDialogProps {
	open: boolean;
	file: FileInfo | FileListItem;
	onClose: () => void;
	onOpenChangeComplete?: (open: boolean) => void;
	onFileUpdated?: () => void;
	downloadPath?: string;
	imagePreviewPath?: string;
	thumbnailPath?: string;
	editable?: boolean;
	previewLinkFactory?: () => Promise<PreviewLinkInfo>;
	archivePreviewFactory?: (options?: {
		signal?: AbortSignal;
	}) => Promise<ArchivePreviewManifest>;
	loadMusicBackendMetadata?: MusicPlayerTrack["loadBackendMetadata"];
	mediaStreamLinkFactory?: () => Promise<ShareStreamSessionInfo>;
	wopiSessionFactory?: (appKey: string) => Promise<WopiLaunchSession>;
	imageNavigation?: {
		nextFile?: FileInfo | FileListItem;
		onNavigate: (file: FileInfo | FileListItem) => void;
		previousFile?: FileInfo | FileListItem;
	};
	openMode?: "auto" | "direct" | "picker";
}

interface FilePreviewDialogModelInput
	extends Omit<
		FilePreviewDialogProps,
		"onOpenChangeComplete" | "onFileUpdated"
	> {
	language?: string;
	translateFileLabel: (key: string) => string;
}

interface DialogState {
	confirmOpen: boolean;
	forceOpenMethodChooser: boolean;
	hasManualExpanded: boolean;
	hasConfirmedInitialMode: boolean;
	isDialogAnimationEnabled: boolean;
	isDirty: boolean;
	isExpanded: boolean;
	mode: OpenWithMode | null;
	showAllOpenMethods: boolean;
}

type DialogStateAction =
	| {
			type: "syncPreferredMode";
			mode: OpenWithMode | null;
			fileChanged: boolean;
	  }
	| { type: "setShowAllOpenMethods"; open: boolean }
	| { type: "setDirty"; dirty: boolean }
	| { type: "setConfirmOpen"; open: boolean }
	| { type: "selectOpenMethod"; mode: OpenWithMode }
	| { type: "openMethodPicker" }
	| { type: "discardChanges" }
	| { type: "setExpanded"; expanded: boolean }
	| { type: "disableDialogAnimation" };

const initialDialogState: DialogState = {
	confirmOpen: false,
	forceOpenMethodChooser: false,
	hasManualExpanded: false,
	hasConfirmedInitialMode: false,
	isDialogAnimationEnabled: true,
	isDirty: false,
	isExpanded: false,
	mode: null,
	showAllOpenMethods: false,
};

function dialogStateReducer(
	state: DialogState,
	action: DialogStateAction,
): DialogState {
	switch (action.type) {
		case "syncPreferredMode":
			if (action.fileChanged) {
				return {
					...state,
					forceOpenMethodChooser: false,
					hasManualExpanded: false,
					hasConfirmedInitialMode: false,
					isExpanded: false,
					mode: action.mode,
				};
			}
			return {
				...state,
				mode: action.mode,
			};
		case "setShowAllOpenMethods":
			return state.showAllOpenMethods === action.open
				? state
				: { ...state, showAllOpenMethods: action.open };
		case "setDirty":
			return state.isDirty === action.dirty
				? state
				: { ...state, isDirty: action.dirty };
		case "setConfirmOpen":
			return state.confirmOpen === action.open
				? state
				: { ...state, confirmOpen: action.open };
		case "selectOpenMethod":
			return {
				...state,
				forceOpenMethodChooser: false,
				hasConfirmedInitialMode: true,
				isDialogAnimationEnabled: true,
				mode: action.mode,
			};
		case "openMethodPicker":
			return {
				...state,
				forceOpenMethodChooser: true,
				hasConfirmedInitialMode: false,
				isDialogAnimationEnabled: true,
				showAllOpenMethods: false,
			};
		case "discardChanges":
			return {
				...state,
				confirmOpen: false,
				isDirty: false,
			};
		// First image auto-expand also marks expansion as manual so later identical
		// actions are skipped and close/open animation does not replay.
		case "setExpanded":
			return state.isExpanded === action.expanded && state.hasManualExpanded
				? state
				: {
						...state,
						hasManualExpanded: true,
						isDialogAnimationEnabled: false,
						isExpanded: action.expanded,
					};
		case "disableDialogAnimation":
			return state.isDialogAnimationEnabled
				? { ...state, isDialogAnimationEnabled: false }
				: state;
	}
}

function getEmbeddedOptionMode(option: OpenWithOption | null) {
	if (!option) {
		return "new_tab";
	}

	if (option.mode !== "url_template" && option.mode !== "wopi") {
		return "iframe";
	}

	return option.config?.mode === "new_tab" ? "new_tab" : "iframe";
}

function useMediaQuery(query: string) {
	const [matches, setMatches] = useState(() =>
		typeof window.matchMedia === "function"
			? window.matchMedia(query).matches
			: false,
	);

	useEffect(() => {
		if (typeof window.matchMedia !== "function") {
			setMatches(false);
			return;
		}

		const mediaQuery = window.matchMedia(query);
		setMatches(mediaQuery.matches);
		const handleChange = () => {
			setMatches(mediaQuery.matches);
		};
		mediaQuery.addEventListener("change", handleChange);
		return () => {
			mediaQuery.removeEventListener("change", handleChange);
		};
	}, [query]);

	return matches;
}

export function useFilePreviewDialogModel({
	open,
	file,
	onClose,
	downloadPath,
	imagePreviewPath,
	thumbnailPath,
	editable = true,
	previewLinkFactory,
	archivePreviewFactory,
	loadMusicBackendMetadata,
	mediaStreamLinkFactory,
	wopiSessionFactory,
	openMode = "auto",
	language,
	translateFileLabel,
}: FilePreviewDialogModelInput) {
	const previewApps = usePreviewAppStore((state) => state.config);
	const isMobilePreviewViewport = useMediaQuery(MOBILE_PREVIEW_MEDIA_QUERY);
	const previewAppsLoaded = usePreviewAppStore((state) => state.isLoaded);
	const loadPreviewApps = usePreviewAppStore((state) => state.load);
	const thumbnailSupport = useThumbnailSupportStore((state) => state.config);
	const thumbnailSupportLoaded = useThumbnailSupportStore(
		(state) => state.isLoaded,
	);
	const loadThumbnailSupport = useThumbnailSupportStore((state) => state.load);
	const mediaDataSupport = useMediaDataSupportStore((state) => state.config);
	const mediaDataSupportLoaded = useMediaDataSupportStore(
		(state) => state.isLoaded,
	);
	const loadMediaDataSupport = useMediaDataSupportStore((state) => state.load);
	const resolvedDownloadPath =
		downloadPath ?? fileService.downloadPath(file.id);
	const resolvedImagePreviewPath =
		imagePreviewPath ?? fileService.imagePreviewPath(file.id);
	const resolvedThumbnailPath =
		thumbnailPath ?? fileService.thumbnailPath(file.id);
	const canRequestAudioMetadata =
		mediaDataSupportLoaded && supportsAudioMediaData(file, mediaDataSupport);
	const resolvedLoadMusicBackendMetadata =
		loadMusicBackendMetadata ??
		(canRequestAudioMetadata
			? (signal?: AbortSignal) =>
					import("@/lib/musicPlayer").then(
						({ backendAudioMetadataToTrackMetadata }) =>
							fileService
								.getMediaMetadata(file.id, { signal })
								.then((metadata) =>
									backendAudioMetadataToTrackMetadata(metadata),
								),
					)
			: undefined);

	useEffect(() => {
		if (!mediaDataSupportLoaded) {
			void loadMediaDataSupport();
		}
	}, [loadMediaDataSupport, mediaDataSupportLoaded]);

	useEffect(() => {
		if (previewAppsLoaded) return;
		void loadPreviewApps();
	}, [loadPreviewApps, previewAppsLoaded]);

	useEffect(() => {
		if (thumbnailSupportLoaded) return;
		void loadThumbnailSupport();
	}, [loadThumbnailSupport, thumbnailSupportLoaded]);

	const baseProfile = useMemo(() => {
		if (!previewAppsLoaded || !thumbnailSupportLoaded) return null;
		return detectFilePreviewProfile(file, previewApps, thumbnailSupport);
	}, [
		file,
		previewApps,
		previewAppsLoaded,
		thumbnailSupport,
		thumbnailSupportLoaded,
	]);

	const customVideoBrowserOption = useMemo(
		() => getVideoBrowserOpenWithOption(),
		[],
	);

	const profile = useMemo(() => {
		if (!baseProfile) return null;
		if (
			baseProfile.category !== "video" ||
			!customVideoBrowserOption ||
			baseProfile.options.some(
				(option) => option.key === customVideoBrowserOption.key,
			)
		) {
			return baseProfile;
		}

		return {
			...baseProfile,
			allOptions: [
				...(baseProfile.allOptions ?? baseProfile.options),
				customVideoBrowserOption,
			],
			options: [...baseProfile.options, customVideoBrowserOption],
		};
	}, [baseProfile, customVideoBrowserOption]);

	const isOptionAvailable = useCallback(
		(option: OpenWithOption) =>
			(option.mode !== "wopi" || Boolean(wopiSessionFactory)) &&
			(option.mode !== "archive" || Boolean(archivePreviewFactory)),
		[archivePreviewFactory, wopiSessionFactory],
	);

	const allOptions = useMemo(
		() =>
			(profile?.allOptions ?? profile?.options ?? []).filter(isOptionAvailable),
		[isOptionAvailable, profile],
	);
	const visibleOptions = useMemo(() => {
		if (!profile || profile.options.length === 0) {
			return allOptions;
		}

		const nextVisibleOptions = profile.options.filter(isOptionAvailable);
		return nextVisibleOptions.length > 0 ? nextVisibleOptions : allOptions;
	}, [allOptions, isOptionAvailable, profile]);
	const hiddenOptions = useMemo(
		() =>
			allOptions.filter(
				(option) =>
					!visibleOptions.some((candidate) => candidate.key === option.key),
			),
		[allOptions, visibleOptions],
	);

	const preferredMode = useMemo(() => {
		if (!profile) return null;
		if (
			profile.defaultMode &&
			allOptions.some((option) => option.key === profile.defaultMode)
		) {
			return profile.defaultMode;
		}
		return allOptions[0]?.key ?? null;
	}, [allOptions, profile]);
	const shouldAutoOpenPreferredMode = useMemo(
		() =>
			openMode === "auto" &&
			Boolean(profile) &&
			profile?.category === "image" &&
			profile.isTextBased &&
			allOptions.some(
				(option) => option.key === preferredMode && option.mode === "image",
			),
		[allOptions, openMode, preferredMode, profile],
	);

	const [state, dispatch] = useReducer(dialogStateReducer, initialDialogState);
	const previousFileIdRef = useRef(file.id);
	const archivePreviewFactoryRef = useRef(archivePreviewFactory);
	const wopiResourceRef = useRef<{
		factory: FilePreviewDialogProps["wopiSessionFactory"];
		key: string;
		resource: WopiSessionResource;
	} | null>(null);

	useEffect(() => {
		archivePreviewFactoryRef.current = archivePreviewFactory;
	}, [archivePreviewFactory]);

	useEffect(() => {
		const hasFileChanged = previousFileIdRef.current !== file.id;
		if (hasFileChanged) {
			previousFileIdRef.current = file.id;
		}
		dispatch({
			type: "syncPreferredMode",
			fileChanged: hasFileChanged,
			mode: preferredMode,
		});
	}, [file.id, preferredMode]);

	const activeMode = state.mode ?? preferredMode;

	useEffect(() => {
		dispatch({
			type: "setShowAllOpenMethods",
			open: Boolean(
				activeMode && hiddenOptions.some((option) => option.key === activeMode),
			),
		});
	}, [activeMode, hiddenOptions]);

	const activeOption = useMemo(() => {
		if (!profile || !activeMode) return null;
		return allOptions.find((option) => option.key === activeMode) ?? null;
	}, [activeMode, allOptions, profile]);

	const getOptionLabel = useCallback(
		(option: OpenWithOption) =>
			resolveOpenWithOptionLabel(option, language, translateFileLabel),
		[language, translateFileLabel],
	);
	const activeWopiSessionFactory = useCallback(() => {
		if (activeOption?.mode !== "wopi" || !wopiSessionFactory) {
			return Promise.reject(new Error("wopi session factory unavailable"));
		}

		return wopiSessionFactory(activeOption.key);
	}, [activeOption, wopiSessionFactory]);
	const activeWopiSessionResource = useMemo(() => {
		if (activeOption?.mode !== "wopi" || !wopiSessionFactory) {
			return null;
		}

		const resourceKey = `${file.id}:${activeOption.key}`;
		if (
			wopiResourceRef.current?.key === resourceKey &&
			wopiResourceRef.current.factory === wopiSessionFactory
		) {
			return wopiResourceRef.current.resource;
		}

		const resource = createWopiSessionResource(() =>
			wopiSessionFactory(activeOption.key),
		);
		wopiResourceRef.current = {
			factory: wopiSessionFactory,
			key: resourceKey,
			resource,
		};
		return resource;
	}, [activeOption, file.id, wopiSessionFactory]);
	const stableArchivePreviewFactory = useCallback(
		(options?: { signal?: AbortSignal }) => {
			const factory = archivePreviewFactoryRef.current;
			if (!factory) {
				return Promise.reject(new Error("archive preview factory unavailable"));
			}

			return factory(options);
		},
		[],
	);
	const activeArchivePreviewFactory =
		open && activeOption?.mode === "archive" && archivePreviewFactory
			? stableArchivePreviewFactory
			: undefined;
	const hasMultipleVisibleOpenMethods = visibleOptions.length > 1;
	const showOpenMethodChooser =
		previewAppsLoaded &&
		(state.forceOpenMethodChooser
			? allOptions.length > 1
			: openMode === "picker"
				? allOptions.length > 1
				: openMode === "direct"
					? false
					: shouldAutoOpenPreferredMode
						? false
						: hasMultipleVisibleOpenMethods) &&
		!state.hasConfirmedInitialMode;

	const usesInnerScroll =
		activeOption?.mode === "pdf" ||
		activeOption?.mode === "table" ||
		((activeOption?.mode === "url_template" || activeOption?.mode === "wopi") &&
			getEmbeddedOptionMode(activeOption) !== "new_tab");
	const fillsViewportHeight =
		activeOption?.mode === "code" ||
		activeOption?.mode === "formatted" ||
		activeOption?.mode === "markdown" ||
		activeOption?.mode === "archive" ||
		activeOption?.mode === "pdf" ||
		activeOption?.mode === "table" ||
		((activeOption?.mode === "url_template" || activeOption?.mode === "wopi") &&
			getEmbeddedOptionMode(activeOption) !== "new_tab");
	const isImagePreview = activeOption?.mode === "image";
	const isExpanded =
		isMobilePreviewViewport ||
		(isImagePreview
			? state.hasManualExpanded
				? state.isExpanded
				: true
			: state.isExpanded);

	const closeWithGuard = useCallback(() => {
		if (state.isDirty) {
			dispatch({ type: "setConfirmOpen", open: true });
			return;
		}
		onClose();
	}, [onClose, state.isDirty]);

	const handleOpenMethodSelect = useCallback((nextMode: OpenWithMode) => {
		dispatch({ type: "selectOpenMethod", mode: nextMode });
	}, []);

	const handleOpenMethodPickerOpen = useCallback(() => {
		dispatch({ type: "openMethodPicker" });
	}, []);

	const handleDiscardChanges = useCallback(() => {
		dispatch({ type: "discardChanges" });
		onClose();
	}, [onClose]);

	const handleExpandToggle = useCallback(() => {
		dispatch({ type: "setExpanded", expanded: !isExpanded });
	}, [isExpanded]);

	useEffect(() => {
		if (!open || showOpenMethodChooser || !state.isDialogAnimationEnabled) {
			return;
		}

		const timer = window.setTimeout(() => {
			dispatch({ type: "disableDialogAnimation" });
		}, PREVIEW_DIALOG_OPEN_ANIMATION_MS);

		return () => {
			window.clearTimeout(timer);
		};
	}, [state.isDialogAnimationEnabled, open, showOpenMethodChooser]);

	const handleDialogOpenChange = useCallback(
		(nextOpen: boolean) => {
			if (nextOpen) {
				return;
			}

			if (showOpenMethodChooser) {
				onClose();
				return;
			}

			closeWithGuard();
		},
		[closeWithGuard, onClose, showOpenMethodChooser],
	);

	const dialogContentClassName = showOpenMethodChooser
		? "flex max-h-[min(90vh,calc(100vh-2rem))] w-[min(96vw,32rem)] max-w-[min(96vw,32rem)] flex-col gap-0 overflow-hidden p-0 sm:max-w-[min(96vw,32rem)]"
		: [
				"flex max-h-[90vh] w-[min(96vw,1200px)] max-w-[min(96vw,1200px)] flex-col gap-0 overflow-hidden p-0 sm:max-w-[min(96vw,1200px)]",
				(fillsViewportHeight || isImagePreview || isExpanded) && "h-[90vh]",
				isImagePreview &&
					"group/image-preview border-zinc-900 bg-zinc-950 shadow-black/35 duration-200 data-open:zoom-in-95 data-closed:zoom-out-95",
				isExpanded &&
					"top-0 left-0 h-screen w-screen max-h-screen max-w-none translate-x-0 translate-y-0 rounded-none sm:max-w-none",
			]
				.filter(Boolean)
				.join(" ");
	const dialogOverlayClassName = isImagePreview
		? "bg-zinc-950/88 duration-200 supports-backdrop-filter:backdrop-blur-xs dark:bg-zinc-950/88"
		: undefined;
	const formattedCategory: "json" | "xml" =
		profile?.category === "xml" || getFileExtension(file) === "xml"
			? "xml"
			: "json";

	return {
		activeArchivePreviewFactory,
		activeMode,
		activeOption,
		allOptions,
		closeWithGuard,
		dialogContentClassName,
		dialogOverlayClassName,
		editable,
		fillsViewportHeight,
		formattedCategory,
		getOptionLabel,
		handleDialogOpenChange,
		handleDiscardChanges,
		handleExpandToggle,
		handleOpenMethodPickerOpen,
		handleOpenMethodSelect,
		hiddenOptions,
		isDirty: state.isDirty,
		isDialogAnimationEnabled: state.isDialogAnimationEnabled,
		isExpanded,
		isImagePreview,
		previewAppsLoaded,
		profile,
		resolvedDownloadPath,
		resolvedImagePreviewPath,
		resolvedLoadMusicBackendMetadata,
		resolvedThumbnailPath,
		setConfirmOpen: (nextOpen: boolean) =>
			dispatch({ type: "setConfirmOpen", open: nextOpen }),
		setIsDirty: (dirty: boolean) => dispatch({ type: "setDirty", dirty }),
		showAllOpenMethods: state.showAllOpenMethods,
		showOpenMethodChooser,
		usesInnerScroll,
		visibleOptions,
		wopiSessionFactory: wopiSessionFactory ? activeWopiSessionFactory : null,
		wopiSessionResource: activeWopiSessionResource,
		onShowAllOpenMethods: () =>
			dispatch({ type: "setShowAllOpenMethods", open: true }),
		confirmOpen: state.confirmOpen,
		mediaStreamLinkFactory,
		previewLinkFactory,
	};
}
