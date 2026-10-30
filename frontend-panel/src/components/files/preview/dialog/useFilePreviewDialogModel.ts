import { useCallback, useReducer } from "react";
import { useFileContentResource } from "@/hooks/useFileResource";
import type { FileResourceDeliveryMode } from "@/lib/resourceRequest";
import type { FileInfo, FileListItem } from "@/types/api";
import { getFileExtension } from "../capabilities/file-capabilities";
import { resolveOpenWithOptionLabel } from "../capabilities/openWithLabel";
import type { OpenWithMode, OpenWithOption } from "../capabilities/types";
import type { FilePreviewResources } from "../resources/filePreviewResources";
import {
	filePreviewDialogUiReducer,
	initialFilePreviewDialogUiState,
} from "./filePreviewDialogState";
import { usePreviewCapabilities } from "./usePreviewCapabilities";
import { usePreviewDialogChromeState } from "./usePreviewDialogChromeState";
import { usePreviewOpenMode } from "./usePreviewOpenMode";
import { usePreviewSessionResources } from "./usePreviewSessionResources";

export interface FilePreviewDialogProps {
	open: boolean;
	file: FileInfo | FileListItem;
	onClose: () => void;
	onFileUpdated?: () => void;
	editable?: boolean;
	resources: FilePreviewResources;
	imageNavigation?: {
		nextFile?: FileInfo | FileListItem;
		onNavigate: (file: FileInfo | FileListItem) => void;
		previousFile?: FileInfo | FileListItem;
	};
	openMode?: "auto" | "direct" | "picker";
}

interface FilePreviewDialogModelInput
	extends Omit<FilePreviewDialogProps, "onFileUpdated"> {
	language?: string;
	translateFileLabel: (key: string) => string;
}

function contentPreviewDeliveryMode(
	option: OpenWithOption | null,
): FileResourceDeliveryMode {
	switch (option?.mode) {
		case "video":
			return "direct_url";
		case "markdown":
		case "table":
		case "formatted":
		case "code":
			return "text";
		default:
			return "blob_url";
	}
}

export function useFilePreviewDialogModel({
	open,
	file,
	onClose,
	editable = true,
	resources,
	openMode = "auto",
	language,
	translateFileLabel,
}: FilePreviewDialogModelInput) {
	const resolvedDownloadPath = resources.paths.download;
	const resolvedImagePreviewPath = resources.paths.imagePreview;
	const resolvedThumbnailPath = resources.paths.thumbnail;
	const archiveManifestLoader = resources.actions?.loadArchiveManifest;
	const createMediaStreamSession = resources.actions?.createMediaStreamSession;
	const createExternalPreviewLink =
		resources.actions?.createExternalPreviewLink;
	const launchWopiSession = resources.actions?.launchWopiSession;
	const {
		allOptions,
		hiddenOptions,
		previewAppsLoaded,
		profile,
		visibleOptions,
	} = usePreviewCapabilities({
		archiveManifestAvailable: Boolean(archiveManifestLoader),
		file,
		wopiSessionAvailable: Boolean(launchWopiSession),
	});
	const [state, dispatch] = useReducer(
		filePreviewDialogUiReducer,
		initialFilePreviewDialogUiState,
	);
	const { activeMode, activeOption, showOpenMethodChooser } =
		usePreviewOpenMode({
			allOptions,
			dispatch,
			fileId: file.id,
			forceOpenMethodChooser: state.forceOpenMethodChooser,
			hasConfirmedInitialMode: state.hasConfirmedInitialMode,
			hiddenOptions,
			openMode,
			previewAppsLoaded,
			profile,
			stateMode: state.mode,
			visibleOptions,
		});
	const contentPreviewNeedsOriginal =
		activeOption?.mode === "pdf" ||
		activeOption?.mode === "video" ||
		activeOption?.mode === "markdown" ||
		activeOption?.mode === "table" ||
		activeOption?.mode === "formatted" ||
		activeOption?.mode === "code";
	const resolvedContentPreviewPath = useFileContentResource({
		deliveryMode: contentPreviewDeliveryMode(activeOption),
		downloadPath: resolvedDownloadPath,
		enabled: contentPreviewNeedsOriginal,
		fileId: file.id,
		mimeType: file.mime_type,
		open,
		resolveResourceHandle: resources.resolve,
	});

	const getOptionLabel = useCallback(
		(option: OpenWithOption) =>
			resolveOpenWithOptionLabel(option, language, translateFileLabel),
		[language, translateFileLabel],
	);
	const {
		activeArchiveManifestLoader,
		launchWopiSession: activeWopiSessionLauncher,
		wopiSessionResource: activeWopiSessionResource,
	} = usePreviewSessionResources({
		activeOption,
		archiveManifestLoader,
		fileId: file.id,
		launchWopiSession,
	});
	const {
		dialogContentClassName,
		dialogOverlayClassName,
		fillsViewportHeight,
		isExpanded,
		isImagePreview,
		usesInnerScroll,
	} = usePreviewDialogChromeState({
		activeOption,
		dispatch,
		hasManualExpanded: state.hasManualExpanded,
		isDialogAnimationEnabled: state.isDialogAnimationEnabled,
		isExpanded: state.isExpanded,
		open,
		showOpenMethodChooser,
	});

	const closeWithGuard = useCallback(() => {
		if (state.isDirty) {
			dispatch({ type: "setConfirmOpen", confirmOpen: true });
			return;
		}
		onClose();
	}, [onClose, state.isDirty]);

	const handleOpenMethodSelect = useCallback((nextMode: OpenWithMode) => {
		dispatch({ type: "selectOpenMethod", mode: nextMode });
	}, []);

	const handleOpenMethodPickerOpen = useCallback(() => {
		dispatch({ type: "openMethodPickerOpened" });
	}, []);

	const handleDiscardChanges = useCallback(() => {
		dispatch({ type: "discardChanges" });
		onClose();
	}, [onClose]);

	const handleExpandToggle = useCallback(() => {
		dispatch({ type: "setExpanded", expanded: !isExpanded });
	}, [isExpanded]);

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

	const formattedCategory: "json" | "xml" =
		profile?.category === "xml" || getFileExtension(file) === "xml"
			? "xml"
			: "json";

	return {
		activeArchiveManifestLoader,
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
		resolvedContentPreviewPath,
		resolvedDownloadPath,
		resolvedImagePreviewPath,
		resolvedThumbnailPath,
		resources,
		setConfirmOpen: (nextOpen: boolean) =>
			dispatch({ type: "setConfirmOpen", confirmOpen: nextOpen }),
		setIsDirty: (dirty: boolean) =>
			dispatch({ type: "setDirty", isDirty: dirty }),
		showAllOpenMethods: state.showAllOpenMethods,
		showOpenMethodChooser,
		usesInnerScroll,
		visibleOptions,
		launchWopiSession: activeWopiSessionLauncher,
		wopiSessionResource: activeWopiSessionResource,
		onShowAllOpenMethods: () => dispatch({ type: "showAllOpenMethods" }),
		confirmOpen: state.confirmOpen,
		createMediaStreamSession,
		createExternalPreviewLink,
	};
}
