import type { KeyboardEvent } from "react";
import { useEffect, useMemo, useState } from "react";
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
	Table,
	TableBody,
	TableCell,
	TableHead,
	TableHeader,
	TableRow,
} from "@/components/ui/table";
import { formatBytes, formatDateTime, formatNumber } from "@/lib/format";
import { cn } from "@/lib/utils";
import { ApiError, ApiPendingError, isRequestCanceled } from "@/services/http";
import type { ArchivePreviewManifest } from "@/types/api";
import { ErrorCode } from "@/types/api-helpers";
import { PreviewError } from "./PreviewError";
import { PreviewLoadingState } from "./PreviewLoadingState";
import { PreviewUnavailable } from "./PreviewUnavailable";

interface ArchivePreviewProps {
	loadManifest?: (options?: {
		signal?: AbortSignal;
	}) => Promise<ArchivePreviewManifest>;
}

type ArchiveEntry = ArchivePreviewManifest["entries"][number];
type ArchivePreviewErrorKind =
	| "disabled"
	| "invalid"
	| "rejected"
	| "sourceTooLarge"
	| "unsupported"
	| "generic";
type ArchiveDirectoryEntry = {
	path: string;
	name: string;
	parent: string | null;
	kind: "directory";
	size: 0;
	compressed_size: 0;
	modified_at: null;
	synthetic: boolean;
};
type ArchiveBrowserEntry = ArchiveEntry | ArchiveDirectoryEntry;
type ArchiveBreadcrumbItem = {
	path: string | null;
	name: string;
};

function classifyArchivePreviewError(error: unknown): ArchivePreviewErrorKind {
	if (!(error instanceof ApiError)) {
		return "generic";
	}

	const subcode = error.subcode ?? "";
	if (
		error.code === ErrorCode.Forbidden &&
		(subcode === "archive_preview.disabled" ||
			subcode === "archive_preview.user_disabled" ||
			subcode === "archive_preview.share_disabled")
	) {
		return "disabled";
	}
	if (
		error.code === ErrorCode.BadRequest &&
		subcode === "archive_preview.unsupported_type"
	) {
		return "unsupported";
	}
	if (
		error.code === ErrorCode.BadRequest &&
		subcode === "archive_preview.source_too_large"
	) {
		return "sourceTooLarge";
	}
	if (
		error.code === ErrorCode.BadRequest &&
		subcode === "archive_preview.invalid_zip"
	) {
		return "invalid";
	}
	if (
		error.code === ErrorCode.BadRequest &&
		(subcode === "archive_preview.rejected" ||
			subcode === "archive_preview.manifest_too_large" ||
			subcode === "archive_preview.source_size_mismatch")
	) {
		return "rejected";
	}

	// Older servers did not attach subcodes yet.
	const message = error.message.toLowerCase();
	if (
		error.code === ErrorCode.Forbidden &&
		message.includes("archive preview") &&
		message.includes("disabled")
	) {
		return "disabled";
	}
	if (
		error.code === ErrorCode.BadRequest &&
		message.includes("archive preview") &&
		message.includes(".zip")
	) {
		return "unsupported";
	}
	if (
		error.code === ErrorCode.BadRequest &&
		message.includes("source archive size") &&
		message.includes("archive preview limit")
	) {
		return "sourceTooLarge";
	}

	return "generic";
}

function entryDepth(path: string) {
	return path.split("/").filter(Boolean).length - 1;
}

function formatZipModifiedAt(value: string | null | undefined) {
	if (!value) return "";
	return formatDateTime(value);
}

function parentPathForArchivePath(path: string) {
	const normalized = path.replace(/\/+$/u, "");
	const slash = normalized.lastIndexOf("/");
	if (slash < 0) return null;
	return normalized.slice(0, slash);
}

function fileNameForArchivePath(path: string) {
	const normalized = path.replace(/\/+$/u, "");
	const slash = normalized.lastIndexOf("/");
	return slash < 0 ? normalized : normalized.slice(slash + 1);
}

function getArchiveEntryPath(entry: ArchiveBrowserEntry) {
	return entry.path.replace(/\/+$/u, "");
}

function buildArchiveDirectoryEntries(
	entries: ArchiveEntry[],
): Map<string, ArchiveDirectoryEntry> {
	const directories = new Map<string, ArchiveDirectoryEntry>();

	const ensureDirectory = (path: string) => {
		const normalized = path.replace(/\/+$/u, "");
		if (!normalized || directories.has(normalized)) return;
		const parent = parentPathForArchivePath(normalized);
		if (parent) {
			ensureDirectory(parent);
		}
		directories.set(normalized, {
			path: normalized,
			name: fileNameForArchivePath(normalized),
			parent,
			kind: "directory",
			size: 0,
			compressed_size: 0,
			modified_at: null,
			synthetic: true,
		});
	};

	for (const entry of entries) {
		const normalizedPath = getArchiveEntryPath(entry);
		if (entry.kind === "directory") {
			ensureDirectory(normalizedPath);
		}
		const parent = entry.parent ?? parentPathForArchivePath(normalizedPath);
		if (parent) {
			ensureDirectory(parent);
		}
	}

	return directories;
}

function displayParentForEntry(entry: ArchiveBrowserEntry) {
	return entry.parent ?? parentPathForArchivePath(getArchiveEntryPath(entry));
}

function compareArchiveEntries(a: ArchiveBrowserEntry, b: ArchiveBrowserEntry) {
	if (a.kind !== b.kind) {
		return a.kind === "directory" ? -1 : 1;
	}
	return a.name.localeCompare(b.name, undefined, {
		numeric: true,
		sensitivity: "base",
	});
}

function buildArchiveBreadcrumb(
	currentFolder: string | null,
	rootName: string,
): ArchiveBreadcrumbItem[] {
	const items: ArchiveBreadcrumbItem[] = [{ path: null, name: rootName }];
	if (!currentFolder) return items;

	const segments = currentFolder.split("/").filter(Boolean);
	let path = "";
	for (const segment of segments) {
		path = path ? `${path}/${segment}` : segment;
		items.push({ path, name: segment });
	}

	return items;
}

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
		return <Icon name="Folder" className="h-4 w-4 shrink-0 text-amber-500" />;
	}

	return (
		<FileTypeIcon
			mimeType="application/octet-stream"
			fileName={entry.name}
			className="h-4 w-4 shrink-0"
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

export function ArchivePreview({ loadManifest }: ArchivePreviewProps) {
	const { t } = useTranslation("files");
	const [manifest, setManifest] = useState<ArchivePreviewManifest | null>(null);
	const [query, setQuery] = useState("");
	const [currentFolder, setCurrentFolder] = useState<string | null>(null);
	const [loading, setLoading] = useState(Boolean(loadManifest));
	const [pending, setPending] = useState(false);
	const [error, setError] = useState<ArchivePreviewErrorKind | null>(null);
	const [reloadKey, setReloadKey] = useState(0);

	useEffect(() => {
		void reloadKey;
		if (!loadManifest) {
			setLoading(false);
			setPending(false);
			setManifest(null);
			return;
		}

		let cancelled = false;
		let retryTimer: number | undefined;
		const controller = new AbortController();
		setLoading(true);
		setError(null);
		loadManifest({ signal: controller.signal })
			.then((nextManifest) => {
				if (!cancelled) {
					setManifest(nextManifest);
					setCurrentFolder(null);
					setPending(false);
				}
			})
			.catch((error: unknown) => {
				if (!cancelled && !isRequestCanceled(error)) {
					if (error instanceof ApiPendingError) {
						setPending(true);
						setLoading(true);
						retryTimer = window.setTimeout(
							() => setReloadKey((value) => value + 1),
							Math.max(1, error.retryAfterSeconds) * 1000,
						);
						return;
					}
					setPending(false);
					setError(classifyArchivePreviewError(error));
				}
			})
			.finally(() => {
				if (!cancelled && retryTimer === undefined) {
					setLoading(false);
				}
			});

		return () => {
			cancelled = true;
			if (retryTimer !== undefined) {
				window.clearTimeout(retryTimer);
			}
			controller.abort();
		};
	}, [loadManifest, reloadKey]);

	const directoryEntries = useMemo(() => {
		if (!manifest) return new Map<string, ArchiveDirectoryEntry>();
		return buildArchiveDirectoryEntries(manifest.entries);
	}, [manifest]);

	const visibleEntries = useMemo(() => {
		if (!manifest) return [];
		const normalized = query.trim().toLowerCase();
		const explicitDirectoryPaths = new Set(
			manifest.entries
				.filter((entry) => entry.kind === "directory")
				.map((entry) => getArchiveEntryPath(entry)),
		);
		const entries: ArchiveBrowserEntry[] = [
			...Array.from(directoryEntries.values()).filter(
				(entry) => !explicitDirectoryPaths.has(entry.path),
			),
			...manifest.entries,
		];

		if (normalized) {
			return entries
				.filter((entry) =>
					getArchiveEntryPath(entry).toLowerCase().includes(normalized),
				)
				.sort(compareArchiveEntries);
		}

		return entries
			.filter((entry) => displayParentForEntry(entry) === currentFolder)
			.sort(compareArchiveEntries);
	}, [currentFolder, directoryEntries, manifest, query]);

	const breadcrumb = useMemo(
		() => buildArchiveBreadcrumb(currentFolder, t("root")),
		[currentFolder, t],
	);
	const isSearching = query.trim().length > 0;
	const currentFolderParent = currentFolder
		? parentPathForArchivePath(currentFolder)
		: null;
	const visibleItemCount = manifest
		? manifest.file_count + manifest.directory_count
		: 0;
	const openArchiveDirectory = (path: string) => {
		setCurrentFolder(path);
		setQuery("");
	};

	if (!loadManifest) {
		return <PreviewUnavailable />;
	}

	if (loading) {
		return (
			<PreviewLoadingState
				text={t(pending ? "archive_preview_generating" : "loading_preview")}
				className="h-full"
			/>
		);
	}

	if (error || !manifest) {
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

		return <PreviewError onRetry={() => setReloadKey((value) => value + 1)} />;
	}

	return (
		<div className="flex h-full min-h-0 w-full min-w-0 flex-col overflow-hidden rounded-xl border border-border/70 bg-card shadow-xs dark:shadow-none">
			<div className="flex shrink-0 flex-wrap items-center gap-2 border-border/60 border-b bg-muted/25 px-3 py-2 dark:bg-muted/15">
				<div className="flex min-w-[14rem] flex-1 items-center gap-2">
					<Icon
						name="MagnifyingGlass"
						className="h-4 w-4 shrink-0 text-muted-foreground"
					/>
					<Input
						type="search"
						aria-label={t("archive_preview_search")}
						placeholder={t("archive_preview_search")}
						value={query}
						onChange={(event) => setQuery(event.target.value)}
						className="h-7 border-border/60 bg-background/70 shadow-none dark:bg-background/25"
					/>
					{query ? (
						<Button
							type="button"
							variant="ghost"
							size="icon-xs"
							aria-label={t("archive_preview_clear_search")}
							onClick={() => setQuery("")}
						>
							<Icon name="X" className="h-3.5 w-3.5" />
						</Button>
					) : null}
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

			{isSearching ? null : (
				<div className="flex shrink-0 items-center gap-2 border-border/60 border-b bg-background/70 px-3 py-2 dark:bg-background/25">
					<Button
						type="button"
						variant="ghost"
						size="icon-xs"
						aria-label={t("archive_preview_parent_folder")}
						title={t("archive_preview_parent_folder")}
						disabled={!currentFolder}
						onClick={() => setCurrentFolder(currentFolderParent)}
					>
						<Icon name="ArrowLeft" className="h-3.5 w-3.5" />
					</Button>
					<Breadcrumb className="min-w-0 flex-1">
						<BreadcrumbList className="min-w-0 gap-1 text-xs">
							{breadcrumb.map((item, index) => {
								const isLast = index === breadcrumb.length - 1;
								return (
									<BreadcrumbItem key={item.path ?? "root"} className="min-w-0">
										{index > 0 ? (
											<BreadcrumbSeparator className="mx-0 text-muted-foreground/45" />
										) : null}
										{isLast ? (
											<BreadcrumbPage className="text-xs">
												{item.name}
											</BreadcrumbPage>
										) : (
											<BreadcrumbLink
												render={
													<button
														type="button"
														onClick={() => setCurrentFolder(item.path)}
													/>
												}
												className="text-xs"
											>
												{item.name}
											</BreadcrumbLink>
										)}
									</BreadcrumbItem>
								);
							})}
						</BreadcrumbList>
					</Breadcrumb>
				</div>
			)}

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
										onOpenDirectory={openArchiveDirectory}
									/>
								))}
							</TableBody>
						</Table>
					</ScrollArea>
				) : (
					<div className="flex h-full min-h-[12rem] items-center justify-center px-6 text-center text-sm text-muted-foreground">
						{query
							? t("archive_preview_no_matches")
							: t("archive_preview_empty")}
					</div>
				)}
			</div>

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
		</div>
	);
}
