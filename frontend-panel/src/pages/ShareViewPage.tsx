import { lazy, Suspense, useCallback, useEffect, useMemo } from "react";
import { useTranslation } from "react-i18next";
import { useNavigate, useParams } from "react-router-dom";
import {
	getImagePreviewNavigation,
	type ImagePreviewNavigation,
} from "@/components/files/preview/navigation/imagePreviewNavigation";
import type { FilePreviewResources } from "@/components/files/preview/resources/filePreviewResources";
import type { ResolveFileResourceHandle } from "@/hooks/useFileResource";
import { usePageTitle } from "@/hooks/usePageTitle";
import { useRetainedDialogValue } from "@/hooks/useRetainedDialogValue";
import { canBrowserRenderImage } from "@/lib/browserImageSupport";
import { derivedFileResource } from "@/lib/fileResource";
import { shareService } from "@/services/shareService";
import { useThumbnailSupportStore } from "@/stores/thumbnailSupportStore";
import type { FileInfo, FileListItem, SharePublicInfo } from "@/types/api";
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

function shareResourcePaths(
	shareType: SharePublicInfo["share_type"],
	token: string,
	fileId?: number,
) {
	if (shareType === "file") {
		return {
			download: shareService.downloadPath(token),
			imagePreview: shareService.imagePreviewPath(token),
			thumbnail: shareService.thumbnailPath(token),
		};
	}

	if (typeof fileId !== "number") {
		throw new Error("share folder file id is required");
	}

	return {
		download: shareService.downloadFolderPath(token, fileId),
		imagePreview: shareService.folderFileImagePreviewPath(token, fileId),
		thumbnail: shareService.folderFileThumbnailPath(token, fileId),
	};
}

export function SharePreviewElement({
	info,
	token,
	previewFile,
	imageNavigation,
	onClose,
	onPreviewNavigate,
}: Pick<
	ReturnType<typeof useShareViewPageController>,
	"info" | "previewFile"
> & {
	token: string;
	imageNavigation?: ImagePreviewNavigation<FileInfo | FileListItem>;
	onClose: () => void;
	onPreviewNavigate: (file: FileInfo | FileListItem) => void;
}) {
	const {
		retainedValue: retainedPreviewFile,
		handleOpenChangeComplete: handlePreviewOpenChangeComplete,
	} = useRetainedDialogValue(previewFile, previewFile !== null);

	const createExternalPreviewLink = useCallback(() => {
		if (!retainedPreviewFile || !info) {
			return Promise.reject(new Error("share preview link is unavailable"));
		}
		return info.share_type === "file"
			? shareService.createPreviewLink(token)
			: shareService.createFolderFilePreviewLink(token, retainedPreviewFile.id);
	}, [info, retainedPreviewFile, token]);

	const loadArchiveManifest = useCallback(
		(options?: Parameters<typeof shareService.getArchivePreview>[1]) => {
			if (!retainedPreviewFile || !info) {
				return Promise.reject(
					new Error("share archive preview is unavailable"),
				);
			}
			return info.share_type === "file"
				? shareService.getArchivePreview(token, options)
				: shareService.getFolderFileArchivePreview(
						token,
						retainedPreviewFile.id,
						options,
					);
		},
		[info, retainedPreviewFile, token],
	);

	const createMediaStreamSession = useCallback(() => {
		if (!retainedPreviewFile || !info) {
			return Promise.reject(new Error("share media stream is unavailable"));
		}
		return info.share_type === "file"
			? shareService.createStreamSession(token)
			: shareService.createFolderFileStreamSession(
					token,
					retainedPreviewFile.id,
				);
	}, [info, retainedPreviewFile, token]);
	const resolveShareResourceHandle = useCallback<ResolveFileResourceHandle>(
		(_fileId, request) => {
			if (!retainedPreviewFile || !info) {
				return Promise.reject(new Error("share resource is unavailable"));
			}

			const {
				download: downloadPath,
				imagePreview: imagePreviewPath,
				thumbnail: thumbnailPath,
			} = shareResourcePaths(info.share_type, token, retainedPreviewFile.id);
			const shouldUseImagePreview =
				request.representation === "image_preview" ||
				(request.representation === "auto" &&
					request.delivery_mode === "blob_url" &&
					retainedPreviewFile.mime_type?.startsWith("image/") &&
					!canBrowserRenderImage(retainedPreviewFile));
			const resourcePath = shouldUseImagePreview
				? imagePreviewPath
				: request.representation === "thumbnail"
					? thumbnailPath
					: downloadPath;

			return Promise.resolve(
				derivedFileResource(resourcePath, {
					deliveryMode: request.delivery_mode,
					mimeType:
						resourcePath === imagePreviewPath || resourcePath === thumbnailPath
							? "image/webp"
							: retainedPreviewFile.mime_type,
					scope: "share",
				}),
			);
		},
		[info, retainedPreviewFile, token],
	);
	const filePreviewImageNavigation = useMemo(
		() =>
			imageNavigation
				? {
						previousFile: imageNavigation.previousFile,
						nextFile: imageNavigation.nextFile,
						onNavigate: onPreviewNavigate,
					}
				: undefined,
		[imageNavigation, onPreviewNavigate],
	);
	const previewResources = useMemo<FilePreviewResources | null>(() => {
		if (!retainedPreviewFile || !info) return null;
		const {
			download: downloadPath,
			imagePreview: imagePreviewPath,
			thumbnail: thumbnailPath,
		} = shareResourcePaths(info.share_type, token, retainedPreviewFile.id);
		return {
			scope: "share",
			paths: {
				download: downloadPath,
				imagePreview: imagePreviewPath,
				thumbnail: thumbnailPath,
			},
			resolve: resolveShareResourceHandle,
			actions: {
				createExternalPreviewLink,
				loadArchiveManifest,
				createMediaStreamSession,
			},
		};
	}, [
		createExternalPreviewLink,
		createMediaStreamSession,
		info,
		loadArchiveManifest,
		resolveShareResourceHandle,
		retainedPreviewFile,
		token,
	]);

	if (!retainedPreviewFile || !previewResources) {
		return null;
	}

	return (
		<Suspense fallback={null}>
			<FilePreview
				file={retainedPreviewFile}
				open={previewFile !== null}
				onClose={onClose}
				onOpenChangeComplete={handlePreviewOpenChangeComplete}
				editable={false}
				resources={previewResources}
				imageNavigation={filePreviewImageNavigation}
			/>
		</Suspense>
	);
}

export default function ShareViewPage() {
	const { t } = useTranslation(["core", "share", "files", "errors"]);
	const { token, folderId: folderIdParam } = useParams<{
		token: string;
		folderId?: string;
	}>();
	const navigate = useNavigate();
	const requestedFolderId = parseShareFolderRoute(folderIdParam);
	const invalidFolderRoute = requestedFolderId === undefined;
	const controller = useShareViewPageController({
		token,
		requestedFolderId: requestedFolderId ?? null,
		enabled: !invalidFolderRoute,
		t,
	});
	const thumbnailSupport = useThumbnailSupportStore((state) => state.config);
	const thumbnailSupportLoaded = useThumbnailSupportStore(
		(state) => state.isLoaded,
	);
	const loadThumbnailSupport = useThumbnailSupportStore((state) => state.load);
	usePageTitle(controller.info?.name ?? t("share:share_mode_page"));
	const closePreview = useCallback(() => {
		controller.setPreviewFile(null);
	}, [controller]);
	const navigateToFolder = useCallback(
		(folderId: number | null) => {
			if (!token) return;
			navigate(
				folderId === null
					? shareService.pagePath(token)
					: shareService.folderPagePath(token, folderId),
			);
		},
		[navigate, token],
	);

	useEffect(() => {
		if (thumbnailSupportLoaded) return;
		void loadThumbnailSupport();
	}, [loadThumbnailSupport, thumbnailSupportLoaded]);
	const previewImageNavigation = useMemo(
		() =>
			controller.info?.share_type === "folder"
				? getImagePreviewNavigation(
						controller.folderContents?.files ?? [],
						controller.previewFile,
						thumbnailSupport,
					)
				: {},
		[
			controller.folderContents?.files,
			controller.info?.share_type,
			controller.previewFile,
			thumbnailSupport,
		],
	);

	if (invalidFolderRoute) {
		return (
			<ShareCenteredPanel
				icon="Warning"
				title={t("unavailable")}
				description={t("errors:folder_not_found")}
			/>
		);
	}

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
			imageNavigation={previewImageNavigation}
			onClose={closePreview}
			onPreviewNavigate={controller.setPreviewFile}
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
							thumbnailSupport?.image_preview?.extensions,
						),
						owner_user_id: null,
						created_by_user_id: null,
						created_by_username: controller.info.shared_by.name,
						team_id: null,
						created_at: new Date().toISOString(),
						updated_at: new Date().toISOString(),
						deleted_at: null,
						is_locked: false,
						tags: [],
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
			selectionShortcutsEnabled={controller.previewFile === null}
			shareOwnerText={shareOwnerText}
			sortBy={controller.sortBy}
			sortOrder={controller.sortOrder}
			token={token}
			viewMode={controller.viewMode}
			onFileDownload={controller.handleFolderFileDownload}
			onFilePreview={controller.handlePreviewFile}
			onNavigateToFolder={navigateToFolder}
			onRefresh={controller.refreshFolder}
			onSortByChange={controller.setSortBy}
			onSortOrderChange={controller.setSortOrder}
			onViewModeChange={controller.setViewMode}
		/>
	);
}

export function parseShareFolderRoute(
	folderId?: string,
): number | null | undefined {
	if (folderId === undefined) return null;
	if (!/^[1-9]\d*$/.test(folderId)) return undefined;
	const value = Number(folderId);
	return Number.isSafeInteger(value) ? value : undefined;
}
