import { Fragment, type KeyboardEvent, useMemo } from "react";
import { useTranslation } from "react-i18next";
import { FileTypeIcon } from "@/components/files/FileTypeIcon";
import { Badge } from "@/components/ui/badge";
import {
	Breadcrumb,
	BreadcrumbItem,
	BreadcrumbLink,
	BreadcrumbList,
	BreadcrumbPage,
	BreadcrumbSeparator,
} from "@/components/ui/breadcrumb";
import { Button } from "@/components/ui/button";
import { Icon } from "@/components/ui/icon";
import { Input } from "@/components/ui/input";
import { ScrollArea } from "@/components/ui/scroll-area";
import {
	Select,
	SelectContent,
	SelectItem,
	SelectTrigger,
	SelectValue,
} from "@/components/ui/select";
import {
	Table,
	TableBody,
	TableCell,
	TableHead,
	TableHeader,
	TableRow,
} from "@/components/ui/table";
import { archiveFilenameEncodingOptions } from "@/lib/archiveFilenameEncoding";
import { formatBytes, formatDateTime, formatNumber } from "@/lib/format";
import { cn } from "@/lib/utils";
import type {
	ArchiveFilenameEncoding,
	ArchivePreviewManifest,
} from "@/types/api";
import type {
	ArchiveBrowserEntry,
	ArchivePreviewErrorKind,
} from "./archivePreviewTypes";
import {
	buildArchiveBreadcrumb,
	displayParentForEntry,
	entryDepth,
	formatZipModifiedAt,
	getArchiveEntryPath,
	parentPathForArchivePath,
} from "./archivePreviewUtils";
import { PreviewError } from "./PreviewError";

function ArchiveSummaryItem({
	label,
	value,
}: {
	label: string;
	value: string;
}) {
	return (
		<span className="inline-flex items-center gap-1.5 whitespace-nowrap">
			<span className="text-muted-foreground">{label}</span>
			<span className="font-medium text-foreground tabular-nums">{value}</span>
		</span>
	);
}

function ArchiveEntryIcon({ entry }: { entry: ArchiveBrowserEntry }) {
	const isDirectory = entry.kind === "directory";
	if (isDirectory) {
		return <Icon name="Folder" className="size-4 shrink-0 text-amber-500" />;
	}

	return (
		<FileTypeIcon
			mimeType="application/octet-stream"
			fileName={entry.name}
			className="size-4 shrink-0"
		/>
	);
}

function ArchiveEntryRow({
	entry,
	searching,
	onOpenDirectory,
}: {
	entry: ArchiveBrowserEntry;
	searching: boolean;
	onOpenDirectory: (path: string) => void;
}) {
	const isDirectory = entry.kind === "directory";
	const parent = displayParentForEntry(entry);
	const path = getArchiveEntryPath(entry);
	const depth = searching ? Math.max(0, entryDepth(path)) : 0;

	const handleOpen = () => {
		if (isDirectory) {
			onOpenDirectory(path);
		}
	};

	const handleKeyDown = (event: KeyboardEvent<HTMLTableRowElement>) => {
		if (!isDirectory) return;
		if (event.key !== "Enter" && event.key !== " ") return;
		event.preventDefault();
		onOpenDirectory(path);
	};

	return (
		<TableRow
			tabIndex={isDirectory ? 0 : undefined}
			className={cn(
				isDirectory &&
					"cursor-pointer focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring/45",
			)}
			onClick={handleOpen}
			onKeyDown={handleKeyDown}
		>
			<TableCell className="max-w-0 pl-3 pr-2 md:pl-3">
				<div className="flex min-w-0 items-center gap-2.5">
					<span
						className="shrink-0"
						style={{ width: `${Math.min(depth, 8) * 12}px` }}
					/>
					<ArchiveEntryIcon entry={entry} />
					<span className="min-w-0 truncate" title={path}>
						{entry.name}
					</span>
					{searching && parent ? (
						<span className="min-w-0 truncate text-xs text-muted-foreground">
							{parent}
						</span>
					) : null}
				</div>
			</TableCell>
			<TableCell className="w-28 text-right text-muted-foreground tabular-nums">
				{isDirectory ? "-" : formatBytes(entry.size)}
			</TableCell>
			<TableCell className="w-40 truncate text-right text-muted-foreground max-md:hidden">
				{formatZipModifiedAt(entry.modified_at)}
			</TableCell>
		</TableRow>
	);
}

function ArchivePreviewToolbar({
	query,
	filenameEncoding,
	manifest,
	visibleItemCount,
	onQueryChange,
	onClearQuery,
	onFilenameEncodingChange,
}: {
	query: string;
	filenameEncoding: ArchiveFilenameEncoding;
	manifest: ArchivePreviewManifest;
	visibleItemCount: number;
	onQueryChange: (query: string) => void;
	onClearQuery: () => void;
	onFilenameEncodingChange: (value: string | null) => void;
}) {
	const { t } = useTranslation("files");

	return (
		<div className="flex shrink-0 flex-wrap items-center gap-2 border-border/60 border-b bg-muted/25 px-3 py-2 dark:bg-muted/15">
			<div className="flex min-w-[14rem] flex-1 items-center gap-2">
				<Icon
					name="MagnifyingGlass"
					className="size-4 shrink-0 text-muted-foreground"
				/>
				<Input
					type="search"
					aria-label={t("archive_preview_search")}
					placeholder={t("archive_preview_search")}
					value={query}
					onChange={(event) => onQueryChange(event.target.value)}
					className="h-7 border-border/60 bg-background/70 shadow-none dark:bg-background/25"
				/>
				{query ? (
					<Button
						type="button"
						variant="ghost"
						size="icon-xs"
						aria-label={t("archive_preview_clear_search")}
						onClick={onClearQuery}
					>
						<Icon name="X" className="size-3.5" />
					</Button>
				) : null}
			</div>
			<div className="flex items-center gap-2">
				<span className="text-muted-foreground text-xs">
					{t("archive_preview_filename_encoding")}
				</span>
				<Select
					value={filenameEncoding}
					onValueChange={onFilenameEncodingChange}
				>
					<SelectTrigger
						aria-label={t("archive_preview_filename_encoding")}
						className="h-7 w-[7.75rem] border-border/60 bg-background/70 text-xs shadow-none dark:bg-background/25"
					>
						<SelectValue />
					</SelectTrigger>
					<SelectContent>
						{archiveFilenameEncodingOptions.map((value) => (
							<SelectItem key={value} value={value}>
								{t(`archive_preview_encoding_${value}`)}
							</SelectItem>
						))}
					</SelectContent>
				</Select>
			</div>
			<div className="flex flex-wrap items-center gap-x-3 gap-y-1 text-xs">
				<ArchiveSummaryItem
					label={t("archive_preview_items")}
					value={formatNumber(visibleItemCount)}
				/>
				<ArchiveSummaryItem
					label={t("archive_preview_files")}
					value={formatNumber(manifest.file_count)}
				/>
				<ArchiveSummaryItem
					label={t("archive_preview_folders")}
					value={formatNumber(manifest.directory_count)}
				/>
				{manifest.truncated ? (
					<Badge variant="outline">{t("archive_preview_truncated")}</Badge>
				) : null}
			</div>
		</div>
	);
}

function ArchiveBreadcrumbBar({
	currentFolder,
	onCurrentFolderChange,
}: {
	currentFolder: string | null;
	onCurrentFolderChange: (currentFolder: string | null) => void;
}) {
	const { t } = useTranslation("files");
	const breadcrumb = useMemo(
		() => buildArchiveBreadcrumb(currentFolder, t("root")),
		[currentFolder, t],
	);
	const currentFolderParent = currentFolder
		? parentPathForArchivePath(currentFolder)
		: null;

	return (
		<div className="flex shrink-0 items-center gap-2 border-border/60 border-b bg-background/70 px-3 py-2 dark:bg-background/25">
			<Button
				type="button"
				variant="ghost"
				size="icon-xs"
				aria-label={t("archive_preview_parent_folder")}
				title={t("archive_preview_parent_folder")}
				disabled={!currentFolder}
				onClick={() => onCurrentFolderChange(currentFolderParent)}
			>
				<Icon name="ArrowLeft" className="size-3.5" />
			</Button>
			<Breadcrumb className="min-w-0 flex-1">
				<BreadcrumbList className="min-w-0 gap-1 text-xs">
					{breadcrumb.map((item, index) => {
						const isLast = index === breadcrumb.length - 1;
						return (
							<Fragment key={item.path ?? "root"}>
								<BreadcrumbItem className="min-w-0">
									{isLast ? (
										<BreadcrumbPage className="text-xs">
											{item.name}
										</BreadcrumbPage>
									) : (
										<BreadcrumbLink
											render={
												<button
													type="button"
													onClick={() => onCurrentFolderChange(item.path)}
												/>
											}
											className="text-xs"
										>
											{item.name}
										</BreadcrumbLink>
									)}
								</BreadcrumbItem>
								{isLast ? null : (
									<BreadcrumbSeparator className="mx-0 text-muted-foreground/45" />
								)}
							</Fragment>
						);
					})}
				</BreadcrumbList>
			</Breadcrumb>
		</div>
	);
}

function ArchiveEntryTable({
	visibleEntries,
	isSearching,
	query,
	onOpenDirectory,
}: {
	visibleEntries: ArchiveBrowserEntry[];
	isSearching: boolean;
	query: string;
	onOpenDirectory: (path: string) => void;
}) {
	const { t } = useTranslation("files");

	return (
		<div className="min-h-0 flex-1">
			{visibleEntries.length > 0 ? (
				<ScrollArea className="h-full bg-background/80 dark:bg-background/25">
					<Table className="table-fixed">
						<TableHeader>
							<TableRow>
								<TableHead className="pl-3 md:pl-3">
									{t("archive_preview_name")}
								</TableHead>
								<TableHead className="w-28 text-right">
									{t("archive_preview_size")}
								</TableHead>
								<TableHead className="w-40 text-right max-md:hidden">
									{t("archive_preview_modified")}
								</TableHead>
							</TableRow>
						</TableHeader>
						<TableBody>
							{visibleEntries.map((entry) => (
								<ArchiveEntryRow
									key={`${entry.kind}:${entry.path}:${entry.size}`}
									entry={entry}
									searching={isSearching}
									onOpenDirectory={onOpenDirectory}
								/>
							))}
						</TableBody>
					</Table>
				</ScrollArea>
			) : (
				<div className="flex h-full min-h-[12rem] items-center justify-center px-6 text-center text-sm text-muted-foreground">
					{query ? t("archive_preview_no_matches") : t("archive_preview_empty")}
				</div>
			)}
		</div>
	);
}

function ArchivePreviewFooter({
	manifest,
}: {
	manifest: ArchivePreviewManifest;
}) {
	const { t } = useTranslation("files");

	return (
		<div className="flex shrink-0 flex-wrap items-center gap-x-4 gap-y-1 border-border/60 border-t bg-muted/20 px-4 py-2 text-xs dark:bg-muted/15">
			<ArchiveSummaryItem
				label={t("archive_preview_uncompressed")}
				value={formatBytes(manifest.total_uncompressed_size)}
			/>
			<ArchiveSummaryItem
				label={t("archive_preview_zip_entries")}
				value={formatNumber(manifest.entry_count)}
			/>
			<span className="text-muted-foreground">
				{t("archive_preview_generated_at", {
					date: formatDateTime(manifest.generated_at),
				})}
			</span>
		</div>
	);
}

export function ArchivePreviewContent({
	manifest,
	query,
	currentFolder,
	filenameEncoding,
	visibleEntries,
	onQueryChange,
	onCurrentFolderChange,
	onOpenDirectory,
	onFilenameEncodingChange,
}: {
	manifest: ArchivePreviewManifest;
	query: string;
	currentFolder: string | null;
	filenameEncoding: ArchiveFilenameEncoding;
	visibleEntries: ArchiveBrowserEntry[];
	onQueryChange: (query: string) => void;
	onCurrentFolderChange: (currentFolder: string | null) => void;
	onOpenDirectory: (path: string) => void;
	onFilenameEncodingChange: (value: string | null) => void;
}) {
	const isSearching = query.trim().length > 0;
	const visibleItemCount = manifest.file_count + manifest.directory_count;

	return (
		<div className="flex h-full min-h-0 w-full min-w-0 flex-col overflow-hidden rounded-xl border border-border/70 bg-card shadow-xs dark:shadow-none">
			<ArchivePreviewToolbar
				query={query}
				filenameEncoding={filenameEncoding}
				manifest={manifest}
				visibleItemCount={visibleItemCount}
				onQueryChange={onQueryChange}
				onClearQuery={() => onQueryChange("")}
				onFilenameEncodingChange={onFilenameEncodingChange}
			/>
			{isSearching ? null : (
				<ArchiveBreadcrumbBar
					currentFolder={currentFolder}
					onCurrentFolderChange={onCurrentFolderChange}
				/>
			)}
			<ArchiveEntryTable
				visibleEntries={visibleEntries}
				isSearching={isSearching}
				query={query}
				onOpenDirectory={onOpenDirectory}
			/>
			<ArchivePreviewFooter manifest={manifest} />
		</div>
	);
}

export function ArchivePreviewErrorState({
	error,
	onRetry,
}: {
	error: ArchivePreviewErrorKind | null;
	onRetry: () => void;
}) {
	if (error === "disabled") {
		return <PreviewError messageKey="archive_preview_disabled" />;
	}
	if (error === "unsupported") {
		return <PreviewError messageKey="archive_preview_unsupported_type" />;
	}
	if (error === "sourceTooLarge") {
		return <PreviewError messageKey="archive_preview_source_too_large" />;
	}
	if (error === "invalid") {
		return <PreviewError messageKey="archive_preview_invalid_zip" />;
	}
	if (error === "rejected") {
		return <PreviewError messageKey="archive_preview_rejected" />;
	}

	return <PreviewError onRetry={onRetry} />;
}
