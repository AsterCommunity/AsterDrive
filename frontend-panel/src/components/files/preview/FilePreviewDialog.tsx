import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import { useTranslation } from "react-i18next";
import { Dialog, DialogContent } from "@/components/ui/dialog";
import { fileService } from "@/services/fileService";
import type { MusicPlayerTrack } from "@/stores/musicPlayerStore";
import { usePreviewAppStore } from "@/stores/previewAppStore";
import type {
	ArchivePreviewManifest,
	FileInfo,
	FileListItem,
	PreviewLinkInfo,
	ShareStreamSessionInfo,
	WopiLaunchSession,
} from "@/types/api";
import { FilePreviewBody } from "./FilePreviewBody";
import { FilePreviewMethodChooser } from "./FilePreviewMethodChooser";
import { FilePreviewPanel } from "./FilePreviewPanel";
import {
	detectFilePreviewProfile,
	getFileExtension,
} from "./file-capabilities";
import { resolveOpenWithOptionLabel } from "./openWithLabel";
import type { OpenWithMode, OpenWithOption } from "./types";
import { UnsavedChangesGuard } from "./UnsavedChangesGuard";
import { getVideoBrowserOpenWithOption } from "./video-browser-config";

const PREVIEW_DIALOG_OPEN_ANIMATION_MS = 120;

interface FilePreviewDialogProps {
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
	openMode?: "auto" | "direct" | "picker";
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

export function FilePreviewDialog({
	open,
	file,
	onClose,
	onOpenChangeComplete,
	onFileUpdated,
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
}: FilePreviewDialogProps) {
	const { i18n, t } = useTranslation(["core", "files"]);
	const previewApps = usePreviewAppStore((state) => state.config);
	const previewAppsLoaded = usePreviewAppStore((state) => state.isLoaded);
	const loadPreviewApps = usePreviewAppStore((state) => state.load);
	const resolvedDownloadPath =
		downloadPath ?? fileService.downloadPath(file.id);
	const resolvedImagePreviewPath =
		imagePreviewPath ?? fileService.imagePreviewPath(file.id);
	const resolvedThumbnailPath =
		thumbnailPath ?? fileService.thumbnailPath(file.id);
	const resolvedLoadMusicBackendMetadata =
		loadMusicBackendMetadata ??
		((signal?: AbortSignal) =>
			import("@/lib/musicPlayer").then(
				({ backendAudioMetadataToTrackMetadata }) =>
					fileService
						.getMediaMetadata(file.id, { signal })
						.then((metadata) => backendAudioMetadataToTrackMetadata(metadata)),
			));

	useEffect(() => {
		if (previewAppsLoaded) return;
		void loadPreviewApps();
	}, [loadPreviewApps, previewAppsLoaded]);

	const baseProfile = useMemo(() => {
		if (!previewAppsLoaded) return null;
		return detectFilePreviewProfile(file, previewApps);
	}, [file, previewApps, previewAppsLoaded]);

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
			options: [...baseProfile.options, customVideoBrowserOption],
			allOptions: [
				...(baseProfile.allOptions ?? baseProfile.options),
				customVideoBrowserOption,
			],
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

	const [mode, setMode] = useState<OpenWithMode | null>(null);
	const [isDialogAnimationEnabled, setIsDialogAnimationEnabled] =
		useState(true);
	const [isExpanded, setIsExpanded] = useState(false);
	const previousFileIdRef = useRef(file.id);
	const archivePreviewFactoryRef = useRef(archivePreviewFactory);
	const [hasConfirmedInitialMode, setHasConfirmedInitialMode] = useState(false);
	const [forceOpenMethodChooser, setForceOpenMethodChooser] = useState(false);
	useEffect(() => {
		archivePreviewFactoryRef.current = archivePreviewFactory;
	}, [archivePreviewFactory]);
	useEffect(() => {
		const hasFileChanged = previousFileIdRef.current !== file.id;
		if (hasFileChanged) {
			previousFileIdRef.current = file.id;
			setHasConfirmedInitialMode(false);
			setIsExpanded(false);
			setForceOpenMethodChooser(false);
		}
		setMode(preferredMode);
	}, [file.id, preferredMode]);

	const [isDirty, setIsDirty] = useState(false);
	const [confirmOpen, setConfirmOpen] = useState(false);
	const activeMode = mode ?? preferredMode;
	const [showAllOpenMethods, setShowAllOpenMethods] = useState(false);
	useEffect(() => {
		setShowAllOpenMethods(
			Boolean(
				activeMode && hiddenOptions.some((option) => option.key === activeMode),
			),
		);
	}, [activeMode, hiddenOptions]);
	const activeOption = useMemo(() => {
		if (!profile || !activeMode) return null;
		return allOptions.find((option) => option.key === activeMode) ?? null;
	}, [activeMode, allOptions, profile]);

	const getOptionLabel = useCallback(
		(option: OpenWithOption) =>
			resolveOpenWithOptionLabel(option, i18n?.language, (key) =>
				t(`files:${key}`),
			),
		[i18n?.language, t],
	);
	const activeWopiSessionFactory = useCallback(() => {
		if (!activeOption || activeOption.mode !== "wopi" || !wopiSessionFactory) {
			return Promise.reject(new Error("wopi session factory unavailable"));
		}

		return wopiSessionFactory(activeOption.key);
	}, [activeOption, wopiSessionFactory]);
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
		(forceOpenMethodChooser
			? allOptions.length > 1
			: openMode === "picker"
				? allOptions.length > 1
				: openMode === "direct"
					? false
					: shouldAutoOpenPreferredMode
						? false
						: hasMultipleVisibleOpenMethods) &&
		!hasConfirmedInitialMode;

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

	const closeWithGuard = useCallback(() => {
		if (isDirty) {
			setConfirmOpen(true);
			return;
		}
		onClose();
	}, [isDirty, onClose]);

	const handleOpenMethodSelect = useCallback((nextMode: OpenWithMode) => {
		setIsDialogAnimationEnabled(true);
		setMode(nextMode);
		setForceOpenMethodChooser(false);
		setHasConfirmedInitialMode(true);
	}, []);

	const handleOpenMethodPickerOpen = useCallback(() => {
		setIsDialogAnimationEnabled(true);
		setForceOpenMethodChooser(true);
		setHasConfirmedInitialMode(false);
		setShowAllOpenMethods(false);
	}, []);

	const handleDiscardChanges = useCallback(() => {
		setConfirmOpen(false);
		setIsDirty(false);
		onClose();
	}, [onClose]);

	const handleExpandToggle = useCallback(() => {
		setIsDialogAnimationEnabled(false);
		setIsExpanded((value) => !value);
	}, []);

	useEffect(() => {
		if (!open || showOpenMethodChooser || !isDialogAnimationEnabled) {
			return;
		}

		const timer = window.setTimeout(() => {
			setIsDialogAnimationEnabled(false);
		}, PREVIEW_DIALOG_OPEN_ANIMATION_MS);

		return () => {
			window.clearTimeout(timer);
		};
	}, [isDialogAnimationEnabled, open, showOpenMethodChooser]);

	const handleDialogOpenChange = useCallback(
		(open: boolean) => {
			if (open) {
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
				(fillsViewportHeight || isExpanded) && "h-[90vh]",
				isExpanded &&
					"top-0 left-0 h-screen w-screen max-h-screen max-w-none translate-x-0 translate-y-0 rounded-none sm:max-w-none",
			]
				.filter(Boolean)
				.join(" ");

	return (
		<>
			<Dialog
				open={open}
				onOpenChange={handleDialogOpenChange}
				onOpenChangeComplete={onOpenChangeComplete}
			>
				<DialogContent
					animated={showOpenMethodChooser ? true : isDialogAnimationEnabled}
					keepMounted
					showCloseButton={false}
					className={dialogContentClassName}
				>
					{showOpenMethodChooser ? (
						<FilePreviewMethodChooser
							file={file}
							activeMode={activeMode}
							allOptions={allOptions}
							visibleOptions={visibleOptions}
							hiddenOptions={hiddenOptions}
							showAllOpenMethods={showAllOpenMethods}
							getOptionLabel={getOptionLabel}
							onClose={onClose}
							onSelect={handleOpenMethodSelect}
							onShowAllOpenMethods={() => setShowAllOpenMethods(true)}
							chooseOpenMethodLabel={t("files:choose_open_method")}
							closeLabel={t("core:close")}
							moreOpenMethodsLabel={t("files:more_open_methods")}
						/>
					) : (
						<FilePreviewPanel
							file={file}
							body={
								<FilePreviewBody
									file={file}
									activeOption={activeOption}
									profile={profile}
									previewAppsLoaded={previewAppsLoaded}
									downloadPath={resolvedDownloadPath}
									imagePreviewPath={resolvedImagePreviewPath}
									thumbnailPath={resolvedThumbnailPath}
									getOptionLabel={getOptionLabel}
									previewLinkFactory={previewLinkFactory}
									archivePreviewFactory={activeArchivePreviewFactory}
									loadMusicBackendMetadata={resolvedLoadMusicBackendMetadata}
									mediaStreamLinkFactory={mediaStreamLinkFactory}
									createWopiSession={
										wopiSessionFactory ? activeWopiSessionFactory : null
									}
									onFileUpdated={onFileUpdated}
									onDirtyChange={setIsDirty}
									editable={editable}
									isExpanded={isExpanded}
									formattedCategory={
										profile?.category === "xml" ||
										getFileExtension(file) === "xml"
											? "xml"
											: "json"
									}
								/>
							}
							allOptionsCount={allOptions.length}
							usesInnerScroll={usesInnerScroll}
							fillsViewportHeight={fillsViewportHeight}
							isExpanded={isExpanded}
							isDirty={isDirty}
							onChooseOpenMethod={handleOpenMethodPickerOpen}
							onToggleExpand={handleExpandToggle}
							onClose={closeWithGuard}
							chooseOpenMethodLabel={t("files:choose_open_method")}
							enterFullscreenLabel={t("files:preview_enter_fullscreen")}
							exitFullscreenLabel={t("files:preview_exit_fullscreen")}
							closeLabel={t("core:close")}
						/>
					)}
				</DialogContent>
			</Dialog>
			<UnsavedChangesGuard
				open={confirmOpen}
				onOpenChange={setConfirmOpen}
				onConfirm={handleDiscardChanges}
			/>
		</>
	);
}
