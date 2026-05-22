import { lazy, Suspense, useCallback, useEffect } from "react";
import { useTranslation } from "react-i18next";
import { useParams } from "react-router-dom";
import { usePageTitle } from "@/hooks/usePageTitle";
import { useRetainedDialogValue } from "@/hooks/useRetainedDialogValue";
import { supportsAudioMediaData } from "@/lib/mediaDataSupport";
import { backendAudioMetadataToTrackMetadata } from "@/lib/musicPlayer";
import { shareService } from "@/services/shareService";
import { useMediaDataSupportStore } from "@/stores/mediaDataSupportStore";
import type { FileInfo } from "@/types/api";
import { ShareFileView } from "./share-view/ShareFileView";
import { ShareLoadingSkeleton } from "./share-view/ShareFolderSkeleton";
import { ShareFolderView } from "./share-view/ShareFolderView";
import { SharePasswordPanel } from "./share-view/SharePasswordPanel";
import { ShareCenteredPanel } from "./share-view/ShareViewShell";
import {
	classifySharedFile,
	compoundExtensionFromName,
	extensionFromName,
} from "./share-view/shareFileClassification";
import { useShareViewPageController } from "./share-view/useShareViewPageController";

const FilePreview = lazy(async () => {
	const module = await import("@/components/files/FilePreview");
	return { default: module.FilePreview };
});

function SharePreviewElement({
	info,
	token,
	previewFile,
	onClose,
}: Pick<
	ReturnType<typeof useShareViewPageController>,
	"info" | "previewFile"
> & {
	token: string;
	onClose: () => void;
}) {
	const mediaDataSupport = useMediaDataSupportStore((state) => state.config);
	const mediaDataSupportLoaded = useMediaDataSupportStore(
		(state) => state.isLoaded,
	);
	const loadMediaDataSupport = useMediaDataSupportStore((state) => state.load);
	const {
		retainedValue: retainedPreviewFile,
		handleOpenChangeComplete: handlePreviewOpenChangeComplete,
	} = useRetainedDialogValue(previewFile, previewFile !== null);

	useEffect(() => {
		if (!mediaDataSupportLoaded) {
			void loadMediaDataSupport();
		}
	}, [loadMediaDataSupport, mediaDataSupportLoaded]);

	const createMediaStreamLink = () => {
		if (!retainedPreviewFile || !info) {
			return Promise.reject(new Error("share media stream is unavailable"));
		}
		return info.share_type === "file"
			? shareService.createStreamSession(token)
			: shareService.createFolderFileStreamSession(
					token,
					retainedPreviewFile.id,
				);
	};

	if (!retainedPreviewFile) {
		return null;
	}

	return (
		<Suspense fallback={null}>
			<FilePreview
				file={retainedPreviewFile}
				open={previewFile !== null}
				onClose={onClose}
				onOpenChangeComplete={handlePreviewOpenChangeComplete}
				downloadPath={
					info?.share_type === "file"
						? shareService.downloadPath(token)
						: shareService.downloadFolderPath(token, retainedPreviewFile.id)
				}
				imagePreviewPath={
					info?.share_type === "file"
						? shareService.imagePreviewPath(token)
						: shareService.folderFileImagePreviewPath(
								token,
								retainedPreviewFile.id,
							)
				}
				thumbnailPath={
					info?.share_type === "file"
						? shareService.thumbnailPath(token)
						: shareService.folderFileThumbnailPath(
								token,
								retainedPreviewFile.id,
							)
				}
				editable={false}
				previewLinkFactory={() =>
					info?.share_type === "file"
						? shareService.createPreviewLink(token)
						: shareService.createFolderFilePreviewLink(
								token,
								retainedPreviewFile.id,
							)
				}
				archivePreviewFactory={(options) =>
					info?.share_type === "file"
						? shareService.getArchivePreview(token, options)
						: shareService.getFolderFileArchivePreview(
								token,
								retainedPreviewFile.id,
								options,
							)
				}
				loadMusicBackendMetadata={
					mediaDataSupportLoaded &&
					supportsAudioMediaData(retainedPreviewFile, mediaDataSupport)
						? (signal) =>
								info?.share_type === "file"
									? shareService
											.getMediaMetadata(token, { signal })
											.then((metadata) =>
												backendAudioMetadataToTrackMetadata(metadata),
											)
									: shareService
											.getFolderFileMediaMetadata(
												token,
												retainedPreviewFile.id,
												{
													signal,
												},
											)
											.then((metadata) =>
												backendAudioMetadataToTrackMetadata(metadata),
											)
						: undefined
				}
				mediaStreamLinkFactory={createMediaStreamLink}
			/>
		</Suspense>
	);
}

export default function ShareViewPage() {
	const { t } = useTranslation(["core", "share", "files", "errors"]);
	const { token } = useParams<{ token: string }>();
	const controller = useShareViewPageController({ token, t });
	usePageTitle(controller.info?.name ?? t("share:share_mode_page"));
	const closePreview = useCallback(() => {
		controller.setPreviewFile(null);
	}, [controller]);

	if (controller.loading) {
		return <ShareLoadingSkeleton />;
	}

	if (controller.error) {
		return (
			<ShareCenteredPanel
				icon="Warning"
				title={t("unavailable")}
				description={controller.error}
			/>
		);
	}

	if (!controller.info) return null;
	if (!token) return null;

	const shareOwnerText = t("share:shared_by", {
		name: controller.info.shared_by.name,
	});
	const previewElement = (
		<SharePreviewElement
			info={controller.info}
			token={token}
			previewFile={controller.previewFile}
			onClose={closePreview}
		/>
	);

	if (controller.needsPassword && !controller.passwordVerified) {
		return (
			<SharePasswordPanel
				info={controller.info}
				password={controller.password}
				shareOwnerText={shareOwnerText}
				onPasswordChange={controller.setPassword}
				onSubmit={controller.handleVerifyPassword}
				t={t}
			/>
		);
	}

	if (controller.info.share_type === "file") {
		const extension = extensionFromName(controller.info.name);
		const compoundExtension = compoundExtensionFromName(controller.info.name);
		const singleShareFile =
			controller.info.mime_type && typeof controller.info.size === "number"
				? ({
						id: -1,
						name: controller.info.name,
						mime_type: controller.info.mime_type,
						size: controller.info.size,
						folder_id: null,
						blob_id: 0,
						extension,
						compound_extension: compoundExtension,
						file_category: classifySharedFile(
							controller.info.name,
							controller.info.mime_type,
							compoundExtension,
						),
						owner_user_id: null,
						created_by_user_id: null,
						created_by_username: controller.info.shared_by.name,
						team_id: null,
						created_at: new Date().toISOString(),
						updated_at: new Date().toISOString(),
						deleted_at: null,
						is_locked: false,
					} satisfies FileInfo)
				: null;

		return (
			<ShareFileView
				info={controller.info}
				previewElement={previewElement}
				shareOwnerText={shareOwnerText}
				singleShareFile={singleShareFile}
				token={token}
				onDownload={controller.handleDownload}
				onPreviewFile={controller.handlePreviewFile}
			/>
		);
	}

	return (
		<ShareFolderView
			breadcrumb={controller.breadcrumb}
			folderContents={controller.folderContents}
			hasMoreFiles={controller.hasMoreFiles}
			info={controller.info}
			loadingMore={controller.loadingMore}
			navigating={controller.navigating}
			previewElement={previewElement}
			sentinelRef={controller.sentinelRef}
			shareOwnerText={shareOwnerText}
			token={token}
			viewMode={controller.viewMode}
			onFileDownload={controller.handleFolderFileDownload}
			onFilePreview={controller.handlePreviewFile}
			onNavigateToFolder={controller.navigateToFolder}
			onViewModeChange={controller.setViewMode}
		/>
	);
}
