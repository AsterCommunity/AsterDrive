import { useEffect, useMemo, useState } from "react";
import { useTranslation } from "react-i18next";
import {
	Dialog,
	DialogContent,
	DialogHeader,
	DialogTitle,
} from "@/components/ui/dialog";
import { ScrollArea } from "@/components/ui/scroll-area";
import { useRetainedDialogValue } from "@/hooks/useRetainedDialogValue";
import { formatBytes, formatDateAbsolute } from "@/lib/format";
import { cn } from "@/lib/utils";
import { fileService } from "@/services/fileService";
import { ApiPendingError } from "@/services/http";
import type {
	FileInfo,
	FileListItem,
	FolderInfo,
	FolderListItem,
	MediaMetadataInfo,
} from "@/types/api";
import { FileInfoDialogContent } from "./file-info-dialog/FileInfoDialogContent";
import {
	formatValueOrFallback,
	hasFileDetails,
	hasFolderDetails,
} from "./file-info-dialog/fileInfoDialogUtils";
import {
	buildMediaMetadataRows,
	mediaMetadataKindForFile,
} from "./file-info-dialog/mediaMetadataRows";
import type { DetailRow } from "./file-info-dialog/types";
import { useMediaQuery } from "./file-info-dialog/useMediaQuery";

interface FileInfoDialogProps {
	open: boolean;
	onOpenChange: (open: boolean) => void;
	file?: FileInfo | FileListItem;
	folder?: FolderInfo | FolderListItem;
	onPreview?: (file: FileInfo | FileListItem) => void;
	onOpenFolder?: (folder: FolderInfo | FolderListItem) => void;
	onShare?: (target: {
		fileId?: number;
		folderId?: number;
		name: string;
		initialMode?: "page" | "direct";
	}) => void;
	onDownload?: (fileId: number, fileName: string) => void;
	onRename?: (type: "file" | "folder", id: number, name: string) => void;
	onVersions?: (fileId: number) => void;
	onToggleLock?: (
		type: "file" | "folder",
		id: number,
		locked: boolean,
	) => Promise<boolean> | boolean | undefined;
}

const DESKTOP_PANEL_EXIT_MS = 220;
const MEDIA_METADATA_PENDING_MAX_RETRIES = 12;
const MEDIA_METADATA_PENDING_MAX_RETRY_DELAY_MS = 30_000;
type FileInfoDialogTarget = {
	file?: FileInfo | FileListItem;
	folder?: FolderInfo | FolderListItem;
};

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

export function FileInfoDialog({
	open,
	onOpenChange,
	file,
	folder,
}: FileInfoDialogProps) {
	const { t } = useTranslation(["files", "core"]);
	const retainedTargetInput = useMemo<FileInfoDialogTarget | null>(
		() => (file ? { file } : folder ? { folder } : null),
		[file, folder],
	);
	const { retainedValue: retainedTarget, handleOpenChangeComplete } =
		useRetainedDialogValue<FileInfoDialogTarget>(retainedTargetInput, open);
	const [resolvedFile, setResolvedFile] = useState<FileInfo | null>(null);
	const [fileDetailsLoading, setFileDetailsLoading] = useState(false);
	const [resolvedFolder, setResolvedFolder] = useState<FolderInfo | null>(null);
	const [folderDetailsLoading, setFolderDetailsLoading] = useState(false);
	const [childCount, setChildCount] = useState<{
		folders: number;
		files: number;
	} | null>(null);
	const [mediaMetadata, setMediaMetadata] = useState<MediaMetadataInfo | null>(
		null,
	);
	const [mediaMetadataLoading, setMediaMetadataLoading] = useState(false);
	const isDesktop = useMediaQuery("(min-width: 1024px)");
	const [desktopMounted, setDesktopMounted] = useState(open);
	const [desktopVisible, setDesktopVisible] = useState(open);
	const renderedFile = file ?? retainedTarget?.file;
	const renderedFolder = folder ?? retainedTarget?.folder;
	const renderedMediaMetadataKind = renderedFile
		? mediaMetadataKindForFile(renderedFile)
		: null;

	useEffect(() => {
		if (!open || !file) {
			setResolvedFile(null);
			setFileDetailsLoading(false);
			return;
		}
		if (hasFileDetails(file)) {
			setResolvedFile(file);
			setFileDetailsLoading(false);
			return;
		}

		let cancelled = false;
		setResolvedFile(null);
		setFileDetailsLoading(true);
		fileService
			.getFile(file.id)
			.then((data) => {
				if (!cancelled) {
					setResolvedFile(data);
				}
			})
			.catch(() => {
				if (!cancelled) {
					setResolvedFile(null);
				}
			})
			.finally(() => {
				if (!cancelled) {
					setFileDetailsLoading(false);
				}
			});

		return () => {
			cancelled = true;
		};
	}, [file, open]);

	useEffect(() => {
		if (!open || !folder) {
			setResolvedFolder(null);
			setFolderDetailsLoading(false);
			return;
		}
		if (hasFolderDetails(folder)) {
			setResolvedFolder(folder);
			setFolderDetailsLoading(false);
			return;
		}

		let cancelled = false;
		setResolvedFolder(null);
		setFolderDetailsLoading(true);
		fileService
			.getFolderInfo(folder.id)
			.then((data) => {
				if (!cancelled) {
					setResolvedFolder(data);
				}
			})
			.catch(() => {
				if (!cancelled) {
					setResolvedFolder(null);
				}
			})
			.finally(() => {
				if (!cancelled) {
					setFolderDetailsLoading(false);
				}
			});

		return () => {
			cancelled = true;
		};
	}, [folder, open]);

	useEffect(() => {
		if (!open || !folder) {
			setChildCount(null);
			return;
		}

		let cancelled = false;
		setChildCount(null);
		fileService
			.listFolder(folder.id, { folder_limit: 0, file_limit: 0 })
			.then((res) => {
				if (!cancelled) {
					setChildCount({
						folders: res.folders_total,
						files: res.files_total,
					});
				}
			})
			.catch(() => {
				if (!cancelled) {
					setChildCount(null);
				}
			});

		return () => {
			cancelled = true;
		};
	}, [open, folder]);

	useEffect(() => {
		if (!open || !renderedFile || !renderedMediaMetadataKind) {
			setMediaMetadata(null);
			setMediaMetadataLoading(false);
			return;
		}

		const controller = new AbortController();
		let retryTimer: number | null = null;
		let cancelled = false;

		const loadMetadata = (attempt: number) => {
			setMediaMetadataLoading(true);
			fileService
				.getMediaMetadata(renderedFile.id, { signal: controller.signal })
				.then((metadata) => {
					if (cancelled || controller.signal.aborted) return;
					setMediaMetadata(metadata);
					setMediaMetadataLoading(false);
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
					setMediaMetadata(null);
					setMediaMetadataLoading(false);
				});
		};

		setMediaMetadata(null);
		loadMetadata(0);

		return () => {
			cancelled = true;
			controller.abort();
			if (retryTimer !== null) {
				window.clearTimeout(retryTimer);
			}
		};
	}, [open, renderedFile, renderedMediaMetadataKind]);

	useEffect(() => {
		if (!isDesktop) {
			setDesktopMounted(open);
			setDesktopVisible(open);
			return;
		}

		let enterTimeout: number | null = null;
		let exitTimeout: number | null = null;

		if (open) {
			setDesktopMounted(true);
			enterTimeout = window.setTimeout(() => {
				setDesktopVisible(true);
			}, 0);
		} else {
			setDesktopVisible(false);
			exitTimeout = window.setTimeout(() => {
				setDesktopMounted(false);
			}, DESKTOP_PANEL_EXIT_MS);
		}

		return () => {
			if (enterTimeout != null) {
				window.clearTimeout(enterTimeout);
			}
			if (exitTimeout != null) {
				window.clearTimeout(exitTimeout);
			}
		};
	}, [isDesktop, open]);

	const activeFile = renderedFile
		? hasFileDetails(renderedFile)
			? renderedFile
			: resolvedFile
		: null;
	const activeFolder = renderedFolder
		? hasFolderDetails(renderedFolder)
			? renderedFolder
			: resolvedFolder
		: null;
	const loadingText = t("info_loading");
	const isShared =
		renderedFile && "is_shared" in renderedFile
			? renderedFile.is_shared
			: renderedFolder && "is_shared" in renderedFolder
				? renderedFolder.is_shared
				: null;

	const title = renderedFile
		? (activeFile ?? renderedFile).name
		: ((activeFolder ?? renderedFolder)?.name ?? "");
	const resolvedLocked = renderedFile
		? (renderedFile.is_locked ?? activeFile?.is_locked ?? false)
		: renderedFolder
			? (renderedFolder.is_locked ?? activeFolder?.is_locked ?? false)
			: false;
	const currentLocked = resolvedLocked;

	const summaryLabel = renderedFile ? t("core:file") : t("core:folder");
	const summarySubtitle = renderedFile
		? formatBytes((activeFile ?? renderedFile).size)
		: childCount != null
			? t("info_children_count", {
					folders: childCount.folders,
					files: childCount.files,
				})
			: folderDetailsLoading
				? loadingText
				: t("core:folder");

	const overviewRows: DetailRow[] = renderedFile
		? [
				{ label: t("info_type"), value: t("core:file") },
				{
					label: t("info_size"),
					value: formatBytes((activeFile ?? renderedFile).size),
				},
				{
					label: t("info_mime"),
					value: (activeFile ?? renderedFile).mime_type,
				},
				{
					label: t("info_created"),
					value: formatValueOrFallback(
						activeFile?.created_at
							? formatDateAbsolute(activeFile.created_at)
							: null,
						fileDetailsLoading,
						loadingText,
					),
				},
				{
					label: t("info_modified"),
					value: formatDateAbsolute((activeFile ?? renderedFile).updated_at),
				},
			]
		: renderedFolder
			? [
					{ label: t("info_type"), value: t("core:folder") },
					{
						label: t("info_children"),
						value:
							childCount != null
								? t("info_children_count", {
										folders: childCount.folders,
										files: childCount.files,
									})
								: loadingText,
					},
					{
						label: t("info_created"),
						value: formatValueOrFallback(
							activeFolder?.created_at
								? formatDateAbsolute(activeFolder.created_at)
								: null,
							folderDetailsLoading,
							loadingText,
						),
					},
					{
						label: t("info_modified"),
						value: formatDateAbsolute(
							(activeFolder ?? renderedFolder).updated_at,
						),
					},
				]
			: [];

	const statusRows: DetailRow[] = [
		{
			label: t("info_locked"),
			value: currentLocked ? t("info_locked_yes") : t("info_locked_no"),
		},
		{
			label: t("info_shared"),
			value:
				isShared == null
					? "—"
					: isShared
						? t("info_shared_yes")
						: t("info_shared_no"),
		},
	];
	const metadataRows =
		renderedMediaMetadataKind != null
			? buildMediaMetadataRows({
					kind: renderedMediaMetadataKind,
					loading: mediaMetadataLoading,
					loadingText,
					metadata: mediaMetadata,
					t,
				})
			: [];
	const metadataTitle = renderedMediaMetadataKind
		? t(`info_media_metadata_${renderedMediaMetadataKind}`)
		: t("info_media_metadata");

	if (
		(isDesktop && !open && !desktopMounted) ||
		(!renderedFile && !renderedFolder)
	) {
		return null;
	}

	const content = (
		<FileInfoDialogContent
			closeLabel={t("close")}
			currentLocked={currentLocked}
			isDesktop={isDesktop}
			isShared={isShared}
			metadataRows={metadataRows}
			metadataTitle={metadataTitle}
			overviewRows={overviewRows}
			overviewTitle={t("info_overview")}
			statusRows={statusRows}
			statusTitle={t("info_status")}
			summaryLabel={summaryLabel}
			summarySubtitle={summarySubtitle}
			targetIcon={
				renderedFile
					? {
							type: "file",
							file: {
								file_category: (activeFile ?? renderedFile).file_category,
								id: (activeFile ?? renderedFile).id,
								mime_type: (activeFile ?? renderedFile).mime_type,
								name: (activeFile ?? renderedFile).name,
							},
						}
					: { type: "folder" }
			}
			title={title}
			onClose={() => onOpenChange(false)}
		/>
	);

	if (isDesktop) {
		return (
			<div
				className={cn(
					"hidden h-full min-h-0 flex-none overflow-hidden transition-[width] duration-280 ease-[cubic-bezier(0.22,1,0.36,1)] motion-reduce:transition-none lg:block",
					desktopVisible ? "w-[22rem]" : "pointer-events-none w-0",
				)}
			>
				<aside
					className={cn(
						"flex h-full min-h-0 w-[22rem] flex-col overflow-hidden border-l bg-muted/20 transition-[opacity,transform] duration-280 ease-[cubic-bezier(0.22,1,0.36,1)] motion-reduce:transition-none",
						desktopVisible
							? "translate-x-0 opacity-100"
							: "translate-x-3 opacity-0",
					)}
					aria-label={t("info")}
				>
					<ScrollArea className="h-full min-h-0 flex-1">{content}</ScrollArea>
				</aside>
			</div>
		);
	}

	return (
		<Dialog
			open={open}
			onOpenChange={onOpenChange}
			onOpenChangeComplete={handleOpenChangeComplete}
		>
			<DialogContent
				keepMounted
				className="max-h-[min(80vh,42rem)] w-[calc(100%-1rem)] max-w-[calc(100%-1rem)] gap-0 overflow-hidden p-0 sm:w-full sm:max-w-lg"
			>
				<DialogHeader className="sr-only">
					<DialogTitle>{title}</DialogTitle>
				</DialogHeader>
				<ScrollArea className="max-h-[min(80vh,42rem)]">{content}</ScrollArea>
			</DialogContent>
		</Dialog>
	);
}
