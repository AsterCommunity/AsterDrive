import { useEffect, useState } from "react";
import { useTranslation } from "react-i18next";
import { Link, useSearchParams } from "react-router-dom";
import { FileTypeIcon } from "@/components/files/FileTypeIcon";
import { FolderIconRenderer } from "@/components/files/FolderIconRenderer";
import { Button } from "@/components/ui/button";
import { Icon } from "@/components/ui/icon";
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
import { handleApiError } from "@/hooks/useApiError";
import { formatBytes } from "@/lib/format";
import { adminContentAccessService } from "@/services/adminContentAccessService";
import { startAdminContentDownload } from "@/services/adminContentDownload";
import type { AdminScopeKind, AdminWorkspaceContents } from "@/types/api";
import { AdminContentAccessGate } from "./AdminContentAccessGate";

const PAGE_SIZE = 50;
function positiveId(value: string | null) {
	const id = Number(value);
	return Number.isSafeInteger(id) && id > 0 ? id : undefined;
}
function WorkspaceContents({
	accessId,
	end,
}: {
	accessId: string;
	end: () => void;
}) {
	const { t } = useTranslation("admin");
	const [search, setSearch] = useSearchParams();
	const [data, setData] = useState<AdminWorkspaceContents | null>(null);
	const [loading, setLoading] = useState(true);
	const [failed, setFailed] = useState(false);
	const [downloading, setDownloading] = useState<number | null>(null);
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
	]);
	const openFolder = (id?: number) =>
		update({
			folderId: id?.toString(),
			folderOffset: undefined,
			fileAfterValue: undefined,
			fileAfterId: undefined,
		});
	const download = async (id: number) => {
		setDownloading(id);
		try {
			await startAdminContentDownload(accessId, id);
		} catch (error) {
			handleApiError(error);
		} finally {
			setDownloading(null);
		}
	};
	const sorts = (
		["name", "size", "created_at", "updated_at", "type"] as const
	).map((value) => ({ value, label: t(`content_sort_${value}`) }));
	return (
		<div className="flex min-h-0 flex-1 flex-col gap-4">
			<div className="flex flex-wrap items-center gap-3">
				<p className="mr-auto text-sm text-muted-foreground">
					{t("content_workspace_read_only")}
				</p>
				<Button variant="outline" onClick={end}>
					{t("content_access_end")}
				</Button>
			</div>
			<nav
				aria-label={t("content_breadcrumb")}
				className="flex flex-wrap items-center gap-2"
			>
				<Button variant="ghost" onClick={() => openFolder()}>
					{t("content_root")}
				</Button>
				{data?.ancestors.map((item) => (
					<Button
						key={item.id}
						variant="ghost"
						onClick={() => openFolder(item.id)}
					>
						{item.name}
					</Button>
				))}
			</nav>
			<div className="flex flex-wrap items-center gap-3">
				<Button
					variant={deleted ? "secondary" : "outline"}
					onClick={() =>
						update({
							deleted: deleted ? undefined : "true",
							folderOffset: undefined,
							fileAfterValue: undefined,
							fileAfterId: undefined,
						})
					}
				>
					{t(deleted ? "content_show_live" : "content_show_deleted")}
				</Button>
				<Select
					items={sorts}
					value={sortBy}
					onValueChange={(value) =>
						update({
							sortBy: value ?? undefined,
							folderOffset: undefined,
							fileAfterValue: undefined,
							fileAfterId: undefined,
						})
					}
				>
					<SelectTrigger aria-label={t("content_sort")} className="w-44">
						<SelectValue />
					</SelectTrigger>
					<SelectContent>
						{sorts.map((item) => (
							<SelectItem key={item.value} value={item.value}>
								{item.label}
							</SelectItem>
						))}
					</SelectContent>
				</Select>
				<Button
					variant="outline"
					onClick={() =>
						update({
							sortOrder: sortOrder === "asc" ? "desc" : "asc",
							folderOffset: undefined,
							fileAfterValue: undefined,
							fileAfterId: undefined,
						})
					}
				>
					{t(sortOrder === "asc" ? "content_ascending" : "content_descending")}
				</Button>
			</div>
			{loading ? (
				<p>{t("core:loading")}</p>
			) : failed ? (
				<p role="alert">{t("content_workspace_unavailable")}</p>
			) : data ? (
				<>
					{data.contents.folders.length + data.contents.files.length === 0 ? (
						<p>{t("content_empty")}</p>
					) : (
						<Table>
							<TableHeader>
								<TableRow>
									<TableHead>{t("content_sort_name")}</TableHead>
									<TableHead>{t("admin_size")}</TableHead>
									<TableHead>{t("content_status")}</TableHead>
									<TableHead>{t("content_actions")}</TableHead>
								</TableRow>
							</TableHeader>
							<TableBody>
								{data.contents.folders.map((item) => (
									<TableRow key={`folder:${item.id}`}>
										<TableCell>
											<Button
												variant="ghost"
												onClick={() => openFolder(item.id)}
											>
												<FolderIconRenderer
													icon={item.icon}
													defaultIcon={
														<Icon name="Folder" className="size-5" />
													}
												/>
												{item.name}
											</Button>
										</TableCell>
										<TableCell>—</TableCell>
										<TableCell>
											{t(deleted ? "content_deleted" : "content_live")}
										</TableCell>
										<TableCell />
									</TableRow>
								))}
								{data.contents.files.map((item) => (
									<TableRow key={`file:${item.id}`}>
										<TableCell>
											<Link
												className="flex items-center gap-2 py-2"
												to={`/admin/files?fileId=${item.id}`}
											>
												<FileTypeIcon
													mimeType={item.mime_type}
													fileName={item.name}
												/>
												{item.name}
											</Link>
										</TableCell>
										<TableCell>{formatBytes(item.size)}</TableCell>
										<TableCell>
											{t(deleted ? "content_deleted" : "content_live")}
										</TableCell>
										<TableCell>
											<Button
												variant="outline"
												disabled={downloading !== null}
												onClick={() => void download(item.id)}
											>
												{t("content_download_current")}
											</Button>
										</TableCell>
									</TableRow>
								))}
							</TableBody>
						</Table>
					)}
					<div className="flex flex-wrap gap-3">
						<Button
							variant="outline"
							disabled={offset === 0}
							onClick={() =>
								update({
									folderOffset: Math.max(0, offset - PAGE_SIZE).toString(),
								})
							}
						>
							{t("content_previous_folders")}
						</Button>
						<Button
							variant="outline"
							disabled={offset + PAGE_SIZE >= data.contents.folders_total}
							onClick={() =>
								update({ folderOffset: (offset + PAGE_SIZE).toString() })
							}
						>
							{t("content_next_folders")}
						</Button>
						<Button
							variant="outline"
							disabled={cursorId === undefined}
							onClick={() =>
								update({ fileAfterValue: undefined, fileAfterId: undefined })
							}
						>
							{t("content_first_files")}
						</Button>
						<Button
							variant="outline"
							disabled={!data.contents.next_file_cursor}
							onClick={() =>
								update({
									fileAfterValue: data.contents.next_file_cursor?.value,
									fileAfterId: data.contents.next_file_cursor?.id.toString(),
								})
							}
						>
							{t("content_next_files")}
						</Button>
					</div>
				</>
			) : null}
		</div>
	);
}

export function AdminWorkspaceBrowser({
	scopeKind,
	targetId,
	targetLabel,
}: {
	scopeKind: AdminScopeKind;
	targetId: number;
	targetLabel?: string;
}) {
	const { t } = useTranslation("admin");
	return (
		<section className="flex min-h-0 flex-1 flex-col gap-4">
			<h2 className="text-lg font-semibold">
				{targetLabel ? `${targetLabel} · ` : ""}
				{t(
					scopeKind === "personal"
						? "content_personal_owner"
						: "content_team_owner",
					{ id: targetId },
				)}
			</h2>
			<AdminContentAccessGate
				key={`${scopeKind}:${targetId}`}
				scopeKind={scopeKind}
				targetId={targetId}
			>
				{(accessId, end) => (
					<WorkspaceContents key={accessId} accessId={accessId} end={end} />
				)}
			</AdminContentAccessGate>
		</section>
	);
}
