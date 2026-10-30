import { type ReactNode, useEffect, useMemo, useState } from "react";
import { useTranslation } from "react-i18next";
import { useSearchParams } from "react-router-dom";
import { SortMenu } from "@/components/common/SortMenu";
import { ToolbarBar } from "@/components/common/ToolbarBar";
import { ViewToggle } from "@/components/common/ViewToggle";
import {
	type FileBrowserContextValue,
	FileBrowserProvider,
} from "@/components/files/FileBrowserContext";
import { FileGrid } from "@/components/files/FileGrid";
import { FileInfoDialog } from "@/components/files/FileInfoDialog";
import { FileTable } from "@/components/files/FileTable";
import { FolderBreadcrumb } from "@/components/files/FolderBreadcrumb";
import { VersionHistoryDialog } from "@/components/files/VersionHistoryDialog";
import { AppLayout } from "@/components/layout/AppLayout";
import { Button } from "@/components/ui/button";
import { Icon } from "@/components/ui/icon";
import { ScrollArea } from "@/components/ui/scroll-area";
import { handleApiError } from "@/hooks/useApiError";
import { adminContentAccessService } from "@/services/adminContentAccessService";
import { startAdminContentDownload } from "@/services/adminContentDownload";
import { useFileStore } from "@/stores/fileStore";
import type {
	AdminScopeKind,
	AdminWorkspaceContents,
	FileInfo,
} from "@/types/api";
import { AdminContentAccessGate } from "./AdminContentAccessGate";
import {
	type AdminWorkspaceIdentity,
	AdminWorkspaceSidebar,
} from "./AdminWorkspaceSidebar";

const PAGE_SIZE = 50;
function positiveId(value: string | null) {
	const id = Number(value);
	return Number.isSafeInteger(id) && id > 0 ? id : undefined;
}
function WorkspaceContents({
	accessId,
	end,
	user,
	team,
}: AdminWorkspaceIdentity & { accessId: string; end: () => void }) {
	const { t } = useTranslation("admin");
	const [search, setSearch] = useSearchParams();
	const [refreshKey, setRefreshKey] = useState(0);
	const [scrollElement, setScrollElement] = useState<HTMLDivElement | null>(
		null,
	);
	const selectedId = positiveId(search.get("fileId"));
	const versionsOpen =
		search.get("panel") === "versions" || search.has("revisionId");
	const [selectedFile, setSelectedFile] = useState<FileInfo | undefined>();
	const [selectedFailed, setSelectedFailed] = useState(false);
	const viewMode = useFileStore((state) => state.viewMode);
	const setViewMode = useFileStore((state) => state.setViewMode);
	const [data, setData] = useState<AdminWorkspaceContents | null>(null);
	const [loading, setLoading] = useState(true);
	const [failed, setFailed] = useState(false);
	const folderId = positiveId(search.get("folderId"));
	const deleted = search.get("deleted") === "true";
	const offset = Math.max(0, Number(search.get("folderOffset")) || 0);
	const cursorValue = search.get("fileAfterValue") ?? undefined;
	const cursorId = positiveId(search.get("fileAfterId"));
	const sortBy =
		(["name", "size", "created_at", "updated_at", "type"] as const).find(
			(value) => value === search.get("sortBy"),
		) ?? "name";
	const sortOrder = search.get("sortOrder") === "desc" ? "desc" : "asc";
	const update = (values: Record<string, string | undefined>) => {
		setSearch((current) => {
			const next = new URLSearchParams(current);
			for (const [key, value] of Object.entries(values)) {
				if (value === undefined) next.delete(key);
				else next.set(key, value);
			}
			return next;
		});
	};
	// biome-ignore lint/correctness/useExhaustiveDependencies: the explicit refresh counter intentionally reloads this resource without changing navigation.
	useEffect(() => {
		let active = true;
		setLoading(true);
		setData(null);
		setFailed(false);
		adminContentAccessService
			.list({
				access_id: accessId,
				folder_id: folderId,
				deleted,
				folder_limit: PAGE_SIZE,
				folder_offset: offset,
				file_limit: PAGE_SIZE,
				file_after_value: cursorValue,
				file_after_id: cursorId,
				sort_by: sortBy,
				sort_order: sortOrder,
			})
			.then((result) => {
				if (active) setData(result);
			})
			.catch((error) => {
				if (active) {
					setFailed(true);
					handleApiError(error);
				}
			})
			.finally(() => {
				if (active) setLoading(false);
			});
		return () => {
			active = false;
		};
	}, [
		accessId,
		folderId,
		deleted,
		offset,
		cursorValue,
		cursorId,
		sortBy,
		sortOrder,
		refreshKey,
	]);
	const openFolder = (id?: number) =>
		update({
			fileId: undefined,
			panel: undefined,
			revisionId: undefined,
			folderId: id?.toString(),
			folderOffset: undefined,
			fileAfterValue: undefined,
			fileAfterId: undefined,
		});
	const download = async (id: number) => {
		try {
			await startAdminContentDownload(accessId, id);
		} catch (error) {
			handleApiError(error);
		}
	};
	const openFile = (id: number, panel = "info") =>
		update({ fileId: String(id), panel, revisionId: undefined });
	const closeFile = () =>
		update({ fileId: undefined, panel: undefined, revisionId: undefined });
	useEffect(() => {
		let active = true;
		setSelectedFile(undefined);
		setSelectedFailed(false);
		if (selectedId == null) return;
		adminContentAccessService
			.getFile(accessId, selectedId)
			.then((detail) => {
				if (!active) return;
				setSelectedFile(detail);
			})
			.catch((error) => {
				if (active) {
					setSelectedFailed(true);
					handleApiError(error);
				}
			});
		return () => {
			active = false;
		};
	}, [accessId, selectedId]);
	const versionSource = useMemo(
		() => ({
			list: (fileId: number, after?: number) =>
				adminContentAccessService.listVersions(accessId, fileId, after),
			download: (revisionId: number) =>
				selectedId == null
					? Promise.reject(new Error("missing file"))
					: startAdminContentDownload(accessId, selectedId, revisionId),
		}),
		[accessId, selectedId],
	);
	const browserContext: FileBrowserContextValue = {
		folders: data?.contents.folders ?? [],
		files: data?.contents.files ?? [],
		browserOpenMode: "single_click",
		readOnly: true,
		selectionEnabled: false,
		contextMenusEnabled: true,
		breadcrumbPathIds: data?.ancestors.map((item) => item.id) ?? [],
		getThumbnailPath: () => null,
		sorting: {
			sortBy,
			sortOrder,
			onSort: (column, order) =>
				update({
					sortBy: column,
					sortOrder: order,
					folderOffset: undefined,
					fileAfterValue: undefined,
					fileAfterId: undefined,
				}),
		},
		onFolderOpen: (id) => openFolder(id),
		onFileClick: (file) => openFile(file.id),
		onDownload: (id) => {
			void download(id);
		},
		onVersions: (id) => openFile(id, "versions"),
		onInfo: (type, id) => {
			if (type === "file") openFile(id);
		},
	};

	return (
		<AppLayout
			sidebarContent={
				<AdminWorkspaceSidebar
					accessId={accessId}
					user={user}
					team={team}
					currentId={folderId}
					onNavigate={openFolder}
					onEnd={end}
				/>
			}
		>
			<ToolbarBar
				left={
					<>
						<Icon
							name={folderId == null ? "House" : "FolderOpen"}
							className="size-4 shrink-0"
						/>
						<FolderBreadcrumb
							items={[
								{ id: null, name: t("files:root") },
								...(data?.ancestors ?? []),
							]}
							onNavigate={(id) => openFolder(id ?? undefined)}
						/>
						<Button
							variant="ghost"
							size="icon"
							aria-label={t("core:refresh")}
							onClick={() => setRefreshKey((value) => value + 1)}
						>
							<Icon name="ArrowsClockwise" className="size-4" />
						</Button>
					</>
				}
				right={
					<>
						<Button
							variant={deleted ? "secondary" : "ghost"}
							size="sm"
							onClick={() =>
								update({
									deleted: deleted ? undefined : "true",
									folderOffset: undefined,
									fileAfterId: undefined,
									fileAfterValue: undefined,
								})
							}
						>
							{t(deleted ? "content_show_live" : "content_show_deleted")}
						</Button>
						<SortMenu
							sortBy={sortBy}
							sortOrder={sortOrder}
							onSortBy={(value) =>
								update({
									sortBy: value,
									folderOffset: undefined,
									fileAfterId: undefined,
									fileAfterValue: undefined,
								})
							}
							onSortOrder={(value) =>
								update({
									sortOrder: value,
									folderOffset: undefined,
									fileAfterId: undefined,
									fileAfterValue: undefined,
								})
							}
						/>
						<ViewToggle value={viewMode} onChange={setViewMode} />
					</>
				}
			/>
			<div className="flex min-h-0 flex-1">
				<ScrollArea ref={setScrollElement} className="min-h-0 min-w-0 flex-1">
					{loading ? (
						<p className="p-5">{t("core:loading")}</p>
					) : failed ? (
						<p role="alert" className="p-5">
							{t("content_workspace_unavailable")}
						</p>
					) : data ? (
						<>
							{data.contents.files.length + data.contents.folders.length ===
							0 ? (
								<p className="p-5">{t("content_empty")}</p>
							) : (
								<FileBrowserProvider value={browserContext}>
									{viewMode === "grid" ? (
										<FileGrid scrollElement={scrollElement} />
									) : (
										<FileTable scrollElement={scrollElement} />
									)}
								</FileBrowserProvider>
							)}
							<div className="flex flex-wrap gap-2 p-4 md:px-5">
								{data.contents.folders_total > PAGE_SIZE ? (
									<>
										<Button
											variant="ghost"
											disabled={offset === 0}
											onClick={() =>
												update({
													folderOffset: Math.max(
														0,
														offset - PAGE_SIZE,
													).toString(),
												})
											}
										>
											{t("content_previous_folders")}
										</Button>
										<Button
											variant="ghost"
											disabled={
												offset + PAGE_SIZE >= data.contents.folders_total
											}
											onClick={() =>
												update({
													folderOffset: (offset + PAGE_SIZE).toString(),
												})
											}
										>
											{t("content_next_folders")}
										</Button>
									</>
								) : null}
								{cursorId != null ? (
									<Button
										variant="ghost"
										onClick={() =>
											update({
												fileAfterValue: undefined,
												fileAfterId: undefined,
											})
										}
									>
										{t("content_first_files")}
									</Button>
								) : null}
								{data.contents.next_file_cursor ? (
									<Button
										variant="ghost"
										onClick={() =>
											update({
												fileAfterValue: data.contents.next_file_cursor?.value,
												fileAfterId:
													data.contents.next_file_cursor?.id.toString(),
											})
										}
									>
										{t("content_next_files")}
									</Button>
								) : null}
							</div>
						</>
					) : null}
				</ScrollArea>
				{selectedId && selectedFailed ? (
					<p role="alert" className="p-5">
						{t("content_file_unavailable")}
					</p>
				) : null}
				{selectedFile ? (
					<FileInfoDialog
						metadataOnly
						file={selectedFile}
						open={!versionsOpen}
						onOpenChange={(open) => {
							if (!open) closeFile();
						}}
						actions={
							<>
								<Button onClick={() => void download(selectedFile.id)}>
									{t("files:download")}
								</Button>
								<Button
									variant="outline"
									onClick={() => openFile(selectedFile.id, "versions")}
								>
									{t("admin:admin_file_versions")}
								</Button>
							</>
						}
					/>
				) : null}
				{selectedFile && versionsOpen ? (
					<VersionHistoryDialog
						open
						onOpenChange={(open) => {
							if (!open) closeFile();
						}}
						fileId={selectedFile.id}
						fileName={selectedFile.name}
						mimeType={selectedFile.mime_type}
						readOnlySource={versionSource}
					/>
				) : null}
			</div>
		</AppLayout>
	);
}

export function AdminWorkspaceBrowser({
	scopeKind,
	targetId,
	user,
	team,
}: AdminWorkspaceIdentity & {
	scopeKind: AdminScopeKind;
	targetId: number;
	targetLabel?: string;
}) {
	const shell = (children: ReactNode) => (
		<AppLayout
			sidebarContent={<AdminWorkspaceSidebar user={user} team={team} />}
		>
			<div className="mx-auto w-full max-w-xl p-5">{children}</div>
		</AppLayout>
	);
	return (
		<AdminContentAccessGate
			key={`${scopeKind}:${targetId}`}
			scopeKind={scopeKind}
			targetId={targetId}
			shell={shell}
		>
			{(accessId, end) => (
				<WorkspaceContents
					key={accessId}
					accessId={accessId}
					end={end}
					user={user}
					team={team}
				/>
			)}
		</AdminContentAccessGate>
	);
}
