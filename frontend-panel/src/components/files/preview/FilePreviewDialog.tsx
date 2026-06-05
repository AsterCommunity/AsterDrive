import { useCallback } from "react";
import { useTranslation } from "react-i18next";
import { Dialog, DialogContent } from "@/components/ui/dialog";
import { FilePreviewBody } from "./FilePreviewBody";
import { FilePreviewMethodChooser } from "./FilePreviewMethodChooser";
import { FilePreviewPanel } from "./FilePreviewPanel";
import { ImagePreviewPanel } from "./ImagePreviewPanel";
import { UnsavedChangesGuard } from "./UnsavedChangesGuard";
import {
	type FilePreviewDialogProps,
	useFilePreviewDialogModel,
} from "./useFilePreviewDialogModel";

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
	const translateFileLabel = useCallback(
		(key: string) => t(`files:${key}`),
		[t],
	);
	const model = useFilePreviewDialogModel({
		open,
		file,
		onClose,
		downloadPath,
		imagePreviewPath,
		thumbnailPath,
		editable,
		previewLinkFactory,
		archivePreviewFactory,
		loadMusicBackendMetadata,
		mediaStreamLinkFactory,
		wopiSessionFactory,
		openMode,
		language: i18n?.language,
		translateFileLabel,
	});

	return (
		<>
			<Dialog
				open={open}
				onOpenChange={model.handleDialogOpenChange}
				onOpenChangeComplete={onOpenChangeComplete}
			>
				<DialogContent
					animated={
						model.showOpenMethodChooser || model.isImagePreview
							? true
							: model.isDialogAnimationEnabled
					}
					keepMounted
					overlayClassName={model.dialogOverlayClassName}
					showCloseButton={false}
					className={model.dialogContentClassName}
				>
					{model.showOpenMethodChooser ? (
						<FilePreviewMethodChooser
							file={file}
							activeMode={model.activeMode}
							allOptions={model.allOptions}
							visibleOptions={model.visibleOptions}
							hiddenOptions={model.hiddenOptions}
							showAllOpenMethods={model.showAllOpenMethods}
							getOptionLabel={model.getOptionLabel}
							onClose={onClose}
							onSelect={model.handleOpenMethodSelect}
							onShowAllOpenMethods={model.onShowAllOpenMethods}
							chooseOpenMethodLabel={t("files:choose_open_method")}
							closeLabel={t("core:close")}
							moreOpenMethodsLabel={t("files:more_open_methods")}
						/>
					) : model.activeOption?.mode === "image" ? (
						<ImagePreviewPanel
							file={file}
							allOptionsCount={model.allOptions.length}
							downloadPath={model.resolvedDownloadPath}
							imagePreviewPath={model.resolvedImagePreviewPath}
							isExpanded={model.isExpanded}
							onChooseOpenMethod={model.handleOpenMethodPickerOpen}
							onToggleExpand={model.handleExpandToggle}
							onClose={model.closeWithGuard}
							chooseOpenMethodLabel={t("files:choose_open_method")}
							enterFullscreenLabel={t("files:preview_enter_fullscreen")}
							exitFullscreenLabel={t("files:preview_exit_fullscreen")}
							closeLabel={t("core:close")}
							fitToWindowLabel={t("files:preview_fit_to_window")}
							previewSourceLabel={t("files:preview_source_preview")}
							originalSourceLabel={t("files:preview_source_original")}
							rotateRightLabel={t("files:preview_rotate_right")}
							zoomInLabel={t("files:preview_zoom_in")}
							zoomOutLabel={t("files:preview_zoom_out")}
						/>
					) : (
						<FilePreviewPanel
							file={file}
							body={
								<FilePreviewBody
									file={file}
									activeOption={model.activeOption}
									profile={model.profile}
									previewAppsLoaded={model.previewAppsLoaded}
									downloadPath={model.resolvedDownloadPath}
									imagePreviewPath={model.resolvedImagePreviewPath}
									thumbnailPath={model.resolvedThumbnailPath}
									getOptionLabel={model.getOptionLabel}
									previewLinkFactory={previewLinkFactory}
									archivePreviewFactory={model.activeArchivePreviewFactory}
									loadMusicBackendMetadata={
										model.resolvedLoadMusicBackendMetadata
									}
									mediaStreamLinkFactory={mediaStreamLinkFactory}
									wopiSessionResource={model.wopiSessionResource}
									onFileUpdated={onFileUpdated}
									onDirtyChange={model.setIsDirty}
									editable={model.editable}
									isExpanded={model.isExpanded}
									formattedCategory={model.formattedCategory}
								/>
							}
							allOptionsCount={model.allOptions.length}
							usesInnerScroll={model.usesInnerScroll}
							fillsViewportHeight={model.fillsViewportHeight}
							isExpanded={model.isExpanded}
							isDirty={model.isDirty}
							onChooseOpenMethod={model.handleOpenMethodPickerOpen}
							onToggleExpand={model.handleExpandToggle}
							onClose={model.closeWithGuard}
							chooseOpenMethodLabel={t("files:choose_open_method")}
							enterFullscreenLabel={t("files:preview_enter_fullscreen")}
							exitFullscreenLabel={t("files:preview_exit_fullscreen")}
							closeLabel={t("core:close")}
						/>
					)}
				</DialogContent>
			</Dialog>
			<UnsavedChangesGuard
				open={model.confirmOpen}
				onOpenChange={model.setConfirmOpen}
				onConfirm={model.handleDiscardChanges}
			/>
		</>
	);
}
