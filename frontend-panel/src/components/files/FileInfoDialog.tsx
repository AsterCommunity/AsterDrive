import { type ReactNode, useMemo, useState } from "react";
import { useTranslation } from "react-i18next";
import { TagChips } from "@/components/files/TagChips";
import {
	TagManagerDialog,
	type TagManagerTarget,
} from "@/components/files/TagManagerDialog";
import { Button } from "@/components/ui/button";
import {
	Dialog,
	DialogContent,
	DialogHeader,
	DialogTitle,
} from "@/components/ui/dialog";
import { Icon } from "@/components/ui/icon";
import { ScrollArea } from "@/components/ui/scroll-area";
import { useRetainedDialogValue } from "@/hooks/useRetainedDialogValue";
import { formatBytes, formatDateAbsolute } from "@/lib/format";
import { cn } from "@/lib/utils";
import type {
	EntityType,
	FileInfo,
	FileListItem,
	FolderInfo,
	FolderListItem,
	TagSummary,
} from "@/types/api";
import { FileInfoDialogContent } from "./file-info-dialog/FileInfoDialogContent";
import {
	formatValueOrFallback,
	hasFileDetails,
	hasFolderDetails,
} from "./file-info-dialog/fileInfoDialogUtils";
import { buildMediaMetadataRows } from "./file-info-dialog/mediaMetadataRows";
import type { DetailRow } from "./file-info-dialog/types";
import { useDesktopInfoPanelMount } from "./file-info-dialog/useDesktopInfoPanelMount";
import { useFileInfoDialogData } from "./file-info-dialog/useFileInfoDialogData";
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

type FileInfoDialogTarget = {
	file?: FileInfo | FileListItem;
	folder?: FolderInfo | FolderListItem;
};

function getSharedFlag({
	renderedFile,
	renderedFolder,
}: {
	renderedFile?: FileInfo | FileListItem;
	renderedFolder?: FolderInfo | FolderListItem;
}) {
	if (renderedFile && "is_shared" in renderedFile) {
		return renderedFile.is_shared;
	}
	if (renderedFolder && "is_shared" in renderedFolder) {
		return renderedFolder.is_shared;
	}
	return null;
}

function buildOverviewRows({
	activeFile,
	activeFolder,
	fileDetailsLoading,
	folderDetailsLoading,
	loadingText,
	renderedFile,
	renderedFolder,
	t,
	childCount,
}: {
	activeFile: FileInfo | null;
	activeFolder: FolderInfo | null;
	childCount: { folders: number; files: number } | null;
	fileDetailsLoading: boolean;
	folderDetailsLoading: boolean;
	loadingText: string;
	renderedFile?: FileInfo | FileListItem;
	renderedFolder?: FolderInfo | FolderListItem;
	t: ReturnType<typeof useTranslation>["t"];
}): DetailRow[] {
	if (renderedFile) {
		const fileStorageUsed = activeFile?.storage_used;
		return [
			{ label: t("info_type"), value: t("core:file") },
			{
				label: t("info_size"),
				value: formatBytes((activeFile ?? renderedFile).size),
			},
			{
				label: t("info_storage_used"),
				value: formatValueOrFallback(
					fileStorageUsed != null ? formatBytes(fileStorageUsed) : null,
					fileDetailsLoading,
					loadingText,
				),
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
		];
	}

	if (!renderedFolder) {
		return [];
	}

	const folderStorageUsed = activeFolder?.storage_used;
	return [
		{ label: t("info_type"), value: t("core:folder") },
		{
			label: t("info_storage_used"),
			value: formatValueOrFallback(
				folderStorageUsed != null ? formatBytes(folderStorageUsed) : null,
				folderDetailsLoading,
				loadingText,
			),
		},
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
			value: formatDateAbsolute((activeFolder ?? renderedFolder).updated_at),
		},
	];
}

function FileInfoDialogFrame({
	content,
	desktopVisible,
	handleOpenChangeComplete,
	isDesktop,
	onOpenChange,
	open,
	title,
}: {
	content: ReactNode;
	desktopVisible: boolean;
	handleOpenChangeComplete: (open: boolean) => void;
	isDesktop: boolean;
	onOpenChange: (open: boolean) => void;
	open: boolean;
	title: string;
}) {
	const { t } = useTranslation(["files", "core"]);

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

function TagsSection({
	entityId,
	entityType,
	initialTags,
	name,
	title,
}: {
	entityId: number;
	entityType: EntityType;
	initialTags: TagSummary[];
	name: string;
	title: string;
}) {
	const { t } = useTranslation(["files"]);
	const [tags, setTags] = useState(initialTags);
	const [managerOpen, setManagerOpen] = useState(false);
	const target = useMemo<TagManagerTarget>(
		() => ({
			mode: "entity",
			entityId,
			entityType,
			initialTags: tags,
			name,
			onTagsChange: setTags,
		}),
		[entityId, entityType, name, tags],
	);

	return (
		<section className="space-y-3 rounded-2xl border border-border/60 bg-card/55 p-4 dark:bg-background/18">
			<div className="flex items-center justify-between gap-3">
				<h3 className="text-[11px] font-semibold uppercase tracking-[0.16em] text-muted-foreground">
					{title}
				</h3>
				<Button
					type="button"
					size="sm"
					variant="outline"
					onClick={() => setManagerOpen(true)}
				>
					<Icon name="Tag" className="size-3.5" />
					{t("tag_manage")}
				</Button>
			</div>
			<TagChips
				tags={tags}
				maxVisible={8}
				empty={
					<p className="text-sm text-muted-foreground">{t("tag_no_tags")}</p>
				}
			/>
			<TagManagerDialog
				open={managerOpen}
				onOpenChange={setManagerOpen}
				target={target}
			/>
		</section>
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
	const renderedFile = file ?? retainedTarget?.file;
	const renderedFolder = folder ?? retainedTarget?.folder;
	const isDesktop = useMediaQuery("(min-width: 1024px)");
	const { desktopMounted, desktopVisible } = useDesktopInfoPanelMount(
		open,
		isDesktop,
	);
	const data = useFileInfoDialogData({
		open,
		renderedFile,
		renderedFolder,
	});
	const activeFile = renderedFile
		? hasFileDetails(renderedFile)
			? renderedFile
			: data.resolvedFile
		: null;
	const activeFolder = renderedFolder
		? hasFolderDetails(renderedFolder)
			? renderedFolder
			: data.resolvedFolder
		: null;
	const loadingText = t("info_loading");
	const isShared = getSharedFlag({ renderedFile, renderedFolder });
	const title = renderedFile
		? (activeFile ?? renderedFile).name
		: ((activeFolder ?? renderedFolder)?.name ?? "");
	const currentLocked = renderedFile
		? (renderedFile.is_locked ?? activeFile?.is_locked ?? false)
		: renderedFolder
			? (renderedFolder.is_locked ?? activeFolder?.is_locked ?? false)
			: false;
	const summaryLabel = renderedFile ? t("core:file") : t("core:folder");
	const summarySubtitle = renderedFile
		? formatBytes((activeFile ?? renderedFile).size)
		: data.childCount != null
			? t("info_children_count", {
					folders: data.childCount.folders,
					files: data.childCount.files,
				})
			: data.folderDetailsLoading
				? loadingText
				: t("core:folder");
	const overviewRows = buildOverviewRows({
		activeFile,
		activeFolder,
		childCount: data.childCount,
		fileDetailsLoading: data.fileDetailsLoading,
		folderDetailsLoading: data.folderDetailsLoading,
		loadingText,
		renderedFile,
		renderedFolder,
		t,
	});
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
		data.renderedMediaMetadataKind != null && data.canRequestMediaMetadata
			? buildMediaMetadataRows({
					kind: data.renderedMediaMetadataKind,
					loading: data.mediaMetadataLoading,
					loadingText,
					metadata: data.mediaMetadata,
					t,
				})
			: [];
	const metadataTitle = data.renderedMediaMetadataKind
		? t(`info_media_metadata_${data.renderedMediaMetadataKind}`)
		: t("info_media_metadata");
	const renderedTarget = renderedFile ?? renderedFolder;
	const activeTarget = activeFile ?? activeFolder ?? renderedTarget;
	const tagsSection =
		activeTarget && (renderedFile || renderedFolder) ? (
			<TagsSection
				key={`${renderedFile ? "file" : "folder"}-${activeTarget.id}`}
				entityId={activeTarget.id}
				entityType={renderedFile ? "file" : "folder"}
				initialTags={activeTarget.tags}
				name={activeTarget.name}
				title={t("info_tags")}
			/>
		) : null;

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
			tagsSection={tagsSection}
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

	return (
		<FileInfoDialogFrame
			content={content}
			desktopVisible={desktopVisible}
			handleOpenChangeComplete={handleOpenChangeComplete}
			isDesktop={isDesktop}
			onOpenChange={onOpenChange}
			open={open}
			title={title}
		/>
	);
}
