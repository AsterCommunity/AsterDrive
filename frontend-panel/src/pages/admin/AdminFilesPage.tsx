import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import { useTranslation } from "react-i18next";
import {
	Link,
	useLocation,
	useNavigationType,
	useSearchParams,
} from "react-router-dom";
import { toast } from "sonner";
import { AdminNumberUnitInput } from "@/components/admin/AdminNumberUnitInput";
import { AdminOffsetPagination } from "@/components/admin/AdminOffsetPagination";
import { AdminSearchFilter } from "@/components/admin/AdminSearchFilter";
import { AdminFileDownloads } from "@/components/admin/content-access/AdminFileDownloads";
import { AdminFilterToolbar } from "@/components/common/AdminFilterToolbar";
import {
	ADMIN_INTERACTIVE_TABLE_ROW_CLASS,
	ADMIN_TABLE_BADGE_CELL_CLASS,
	ADMIN_TABLE_MONO_TEXT_CLASS,
	ADMIN_TABLE_STACKED_CELL_CLASS,
	ADMIN_TABLE_TEXT_CELL_CLASS,
	AdminSortableTableHead,
	AdminTableCell as TableCell,
	AdminTableHead as TableHead,
	AdminTableHeader as TableHeader,
	AdminTableRow as TableRow,
} from "@/components/common/AdminTable";
import { AdminTableList } from "@/components/common/AdminTableList";
import { ConfirmDialog } from "@/components/common/ConfirmDialog";
import { UserIdentity } from "@/components/common/UserIdentity";
import { UserIdentityGroup } from "@/components/common/UserIdentityGroup";
import { AdminLayout } from "@/components/layout/AdminLayout";
import { AdminPageHeader } from "@/components/layout/AdminPageHeader";
import { AdminPageShell } from "@/components/layout/AdminPageShell";
import { Badge } from "@/components/ui/badge";
import { Button } from "@/components/ui/button";
import {
	Dialog,
	DialogContent,
	DialogHeader,
	DialogTitle,
} from "@/components/ui/dialog";
import { DialogLifecycle } from "@/components/ui/dialog-lifecycle";
import {
	DropdownMenu,
	DropdownMenuContent,
	DropdownMenuItem,
	DropdownMenuTrigger,
} from "@/components/ui/dropdown-menu";
import { Icon } from "@/components/ui/icon";
import { Input } from "@/components/ui/input";
import {
	Select,
	SelectContent,
	SelectItem,
	SelectTrigger,
	SelectValue,
} from "@/components/ui/select";
import { handleApiError } from "@/hooks/useApiError";
import { useDebouncedQueryDraft } from "@/hooks/useDebouncedQueryDraft";
import {
	useManagedAdminList,
	useManagedAdminListDetailDialog,
	useManagedOffset,
} from "@/hooks/useManagedAdminList";
import {
	type ManagedListQuerySchema,
	useManagedListQueryState,
} from "@/hooks/useManagedListQueryState";
import { usePageTitle } from "@/hooks/usePageTitle";
import {
	BYTE_FILTER_UNITS,
	type ByteFilterUnit,
	formatByteSizeInput,
	parseByteFilterUnit,
	parseByteSizeInput,
} from "@/lib/byteSizeFilter";
import { ADMIN_CONTROL_HEIGHT_CLASS } from "@/lib/constants";
import { formatBytes, formatDateAbsoluteWithOffset } from "@/lib/format";
import {
	parseOffsetSearchParam,
	parsePageSizeOption,
	parsePageSizeSearchParam,
	parseSortOrderSearchParam,
	parseSortSearchParam,
	type SortOrder,
} from "@/lib/pagination";
import { adminFileService } from "@/services/adminService";
import type { AdminFileBlobSortBy, AdminFileSortBy } from "@/types/adminSort";
import type {
	AdminFileBlobDetail,
	AdminFileBlobHashKind,
	AdminFileBlobHealth,
	AdminFileBlobInfo,
	AdminFileDetail,
	AdminFileInfo,
	BlobMaintenanceAction,
} from "@/types/api";

type AdminFilesPageKind = "files" | "blobs";
type DeletedFilter = "__all__" | "live" | "deleted";
type ManagedFileQuery = {
	uploaderUserId?: number;
	health: AdminFileBlobHealth | "__all__";
	hashKind: AdminFileBlobHashKind | "__all__";
	sizeMinUnit: ByteFilterUnit;
	sizeMaxUnit: ByteFilterUnit;
	deleted: DeletedFilter;
	offset: number;
	ownerUserId?: number;
	pageSize: (typeof PAGE_SIZE_OPTIONS)[number];
	policyId?: number;
	refCountMax?: number;
	refCountMin?: number;
	secondaryId?: number;
	sizeMax?: number;
	sizeMin?: number;
	sortBy: AdminFileSortBy | AdminFileBlobSortBy;
	sortOrder: SortOrder;
	storagePath: string;
	teamId?: number;
	text: string;
};

const BLOB_HEALTH_OPTIONS = [
	"healthy",
	"orphan",
	"ref_count_mismatch",
	"cleanup_claimed",
] as const;
const BLOB_HASH_OPTIONS = ["content_sha256", "opaque"] as const;

const PAGE_SIZE_OPTIONS = [20, 50, 100] as const;
const DEFAULT_PAGE_SIZE = 20 as const;
const FILE_SORT_OPTIONS = [
	"id",
	"name",
	"size",
	"blob_id",
	"policy_id",
	"owner_user_id",
	"team_id",
	"created_at",
	"updated_at",
	"deleted_at",
] as const satisfies readonly AdminFileSortBy[];
const BLOB_SORT_OPTIONS = [
	"id",
	"hash",
	"size",
	"policy_id",
	"storage_path",
	"ref_count",
	"created_at",
	"updated_at",
] as const satisfies readonly AdminFileBlobSortBy[];
const DEFAULT_FILE_SORT_BY = "created_at" as const satisfies AdminFileSortBy;
const DEFAULT_BLOB_SORT_BY =
	"created_at" as const satisfies AdminFileBlobSortBy;
const DEFAULT_SORT_ORDER = "desc" as const satisfies SortOrder;

function normalizeOffset(offset: number) {
	return Math.max(0, Math.floor(offset));
}

function parseOptionalNumber(value: string | null) {
	if (value == null || value.trim() === "") return undefined;
	const parsed = Number(value);
	return Number.isSafeInteger(parsed) ? parsed : undefined;
}

function optionalNumberValue(value: number | undefined) {
	return value == null ? "" : String(value);
}

function parseDeletedFilter(value: string | null): DeletedFilter {
	return value === "live" || value === "deleted" ? value : "__all__";
}

function deletedToQuery(value: DeletedFilter) {
	if (value === "live") return false;
	if (value === "deleted") return true;
	return undefined;
}

function deletedFilterLabelKey(value: DeletedFilter) {
	if (value === "live") return "admin_deleted_live";
	if (value === "deleted") return "admin_deleted_deleted";
	return "admin_deleted_all";
}

function hashPreview(hash?: string | null) {
	if (!hash) return "-";
	return hash.length > 18 ? `${hash.slice(0, 10)}...${hash.slice(-6)}` : hash;
}

function displayValue(value: string | number | null | undefined) {
	return value == null || value === "" ? "-" : value;
}

function blobHealthLabelKey(health: AdminFileBlobHealth) {
	return `admin_blob_health_${health}` as const;
}

function blobHealthVariant(health: AdminFileBlobHealth) {
	return health === "healthy" ? "outline" : "destructive";
}

function fileBlobSummary(file: AdminFileInfo | null) {
	return file?.blob;
}

function createManagedFileQueryDefaults(
	kind: AdminFilesPageKind,
): ManagedFileQuery {
	const isFiles = kind === "files";
	return {
		uploaderUserId: undefined,
		health: "__all__",
		hashKind: "__all__",
		sizeMinUnit: "B",
		sizeMaxUnit: "B",
		deleted: "__all__",
		offset: 0,
		ownerUserId: undefined,
		pageSize: DEFAULT_PAGE_SIZE,
		policyId: undefined,
		refCountMax: undefined,
		refCountMin: undefined,
		secondaryId: undefined,
		sizeMax: undefined,
		sizeMin: undefined,
		sortBy: isFiles ? DEFAULT_FILE_SORT_BY : DEFAULT_BLOB_SORT_BY,
		sortOrder: DEFAULT_SORT_ORDER,
		storagePath: "",
		teamId: undefined,
		text: "",
	};
}

function createManagedFileQuerySchema(
	kind: AdminFilesPageKind,
): ManagedListQuerySchema<ManagedFileQuery> {
	const isFiles = kind === "files";
	const defaultSortBy = isFiles ? DEFAULT_FILE_SORT_BY : DEFAULT_BLOB_SORT_BY;

	return {
		uploaderUserId: {
			keys: ["uploaderUserId"],
			parse: (params) => parseOptionalNumber(params.get("uploaderUserId")),
			serialize: (value) => (!isFiles ? value : undefined),
		},
		health: {
			keys: ["health"],
			parse: (params) =>
				BLOB_HEALTH_OPTIONS.includes(
					params.get("health") as AdminFileBlobHealth,
				)
					? (params.get("health") as AdminFileBlobHealth)
					: "__all__",
			serialize: (value) =>
				!isFiles && value !== "__all__" ? value : undefined,
		},
		hashKind: {
			keys: ["hashKind"],
			parse: (params) =>
				BLOB_HASH_OPTIONS.includes(
					params.get("hashKind") as AdminFileBlobHashKind,
				)
					? (params.get("hashKind") as AdminFileBlobHashKind)
					: "__all__",
			serialize: (value) =>
				!isFiles && value !== "__all__" ? value : undefined,
		},
		sizeMinUnit: {
			keys: ["sizeMinUnit", "sizeUnit"],
			parse: (params) =>
				parseByteFilterUnit(
					params.get("sizeMinUnit") ?? params.get("sizeUnit"),
				),
			serialize: (value) =>
				!isFiles && value !== "B" ? { sizeMinUnit: value } : undefined,
		},
		sizeMaxUnit: {
			keys: ["sizeMaxUnit"],
			parse: (params) =>
				parseByteFilterUnit(
					params.get("sizeMaxUnit") ?? params.get("sizeUnit"),
				),
			serialize: (value) => (!isFiles && value !== "B" ? value : undefined),
		},

		deleted: {
			keys: ["deleted"],
			parse: (searchParams) => parseDeletedFilter(searchParams.get("deleted")),
			serialize: (value) =>
				isFiles && value !== "__all__" ? { deleted: value } : undefined,
		},
		offset: {
			keys: ["offset"],
			parse: (searchParams) =>
				normalizeOffset(parseOffsetSearchParam(searchParams.get("offset"))),
			serialize: (value) => (value > 0 ? value : undefined),
		},
		ownerUserId: {
			keys: ["ownerUserId"],
			parse: (searchParams) =>
				parseOptionalNumber(searchParams.get("ownerUserId")),
			serialize: (value) => (isFiles ? value : undefined),
		},
		pageSize: {
			keys: ["pageSize"],
			parse: (searchParams) =>
				parsePageSizeSearchParam(
					searchParams.get("pageSize"),
					PAGE_SIZE_OPTIONS,
					DEFAULT_PAGE_SIZE,
				),
			serialize: (value) =>
				value !== DEFAULT_PAGE_SIZE ? { pageSize: value } : undefined,
		},
		policyId: {
			keys: ["policyId"],
			parse: (searchParams) =>
				parseOptionalNumber(searchParams.get("policyId")),
		},
		refCountMax: {
			keys: ["refCountMax"],
			parse: (searchParams) =>
				parseOptionalNumber(searchParams.get("refCountMax")),
			serialize: (value) => (!isFiles ? value : undefined),
		},
		refCountMin: {
			keys: ["refCountMin"],
			parse: (searchParams) =>
				parseOptionalNumber(searchParams.get("refCountMin")),
			serialize: (value) => (!isFiles ? value : undefined),
		},
		secondaryId: {
			keys: isFiles ? ["blobId"] : [],
			parse: (searchParams) =>
				isFiles ? parseOptionalNumber(searchParams.get("blobId")) : undefined,
			serialize: (value) => (isFiles ? { blobId: value } : undefined),
		},
		sizeMax: {
			keys: ["sizeMax"],
			parse: (searchParams) => parseOptionalNumber(searchParams.get("sizeMax")),
			serialize: (value) => (!isFiles ? value : undefined),
		},
		sizeMin: {
			keys: ["sizeMin"],
			parse: (searchParams) => parseOptionalNumber(searchParams.get("sizeMin")),
			serialize: (value) => (!isFiles ? value : undefined),
		},
		sortBy: {
			keys: ["sortBy"],
			parse: (searchParams) =>
				isFiles
					? parseSortSearchParam(
							searchParams.get("sortBy"),
							FILE_SORT_OPTIONS,
							DEFAULT_FILE_SORT_BY,
						)
					: parseSortSearchParam(
							searchParams.get("sortBy"),
							BLOB_SORT_OPTIONS,
							DEFAULT_BLOB_SORT_BY,
						),
			serialize: (value) =>
				value !== defaultSortBy ? { sortBy: value } : undefined,
		},
		sortOrder: {
			keys: ["sortOrder"],
			parse: (searchParams) =>
				parseSortOrderSearchParam(
					searchParams.get("sortOrder"),
					DEFAULT_SORT_ORDER,
				),
			serialize: (value) =>
				value !== DEFAULT_SORT_ORDER ? { sortOrder: value } : undefined,
		},
		storagePath: {
			keys: ["storagePath"],
			parse: (searchParams) => searchParams.get("storagePath") ?? "",
			serialize: (value) =>
				!isFiles ? { storagePath: value.trim() || undefined } : undefined,
		},
		teamId: {
			keys: ["teamId"],
			parse: (searchParams) => parseOptionalNumber(searchParams.get("teamId")),
			serialize: (value) => (isFiles ? value : undefined),
		},
		text: {
			keys: ["name", "hash"],
			parse: (searchParams) =>
				searchParams.get(isFiles ? "name" : "hash") ?? "",
			serialize: (value) => ({
				[isFiles ? "name" : "hash"]: value.trim() || undefined,
			}),
		},
	};
}

function DetailRow({
	label,
	value,
}: {
	label: string;
	value: React.ReactNode;
}) {
	return (
		<div className="grid grid-cols-[140px_minmax(0,1fr)] gap-3 border-b border-border/50 py-2 text-sm last:border-b-0">
			<div className="text-muted-foreground">{label}</div>
			<div className="min-w-0 break-all font-medium text-foreground">
				{value}
			</div>
		</div>
	);
}

function FileDetailDialog(props: {
	file: AdminFileDetail | null;
	revisionId?: number;
	open: boolean;
	onOpenChange: (open: boolean) => void;
}) {
	return (
		<DialogLifecycle
			open={props.open}
			target={props}
			onOpenChange={props.onOpenChange}
		>
			{({ target, active, onOpenChange }) => (
				<FileDetailDialogContent
					{...target}
					open={active}
					onOpenChange={onOpenChange}
				/>
			)}
		</DialogLifecycle>
	);
}

function FileDetailDialogContent({
	revisionId,
	file: currentFile,
	open,
	onOpenChange,
}: {
	file: AdminFileDetail | null;
	revisionId?: number;
	open: boolean;
	onOpenChange: (open: boolean) => void;
}) {
	const file = currentFile;
	const selectedRevisionId = revisionId;
	const { t } = useTranslation("admin");
	return (
		<Dialog open={open} onOpenChange={onOpenChange}>
			<DialogContent className="max-h-[min(860px,calc(100vh-2rem))] overflow-y-auto sm:max-w-[760px]">
				<DialogHeader>
					<DialogTitle>{file?.name ?? t("admin_file_detail")}</DialogTitle>
				</DialogHeader>
				{file ? (
					<div className="space-y-4">
						<AdminFileDownloads
							key={`${file.id}:${selectedRevisionId ?? "current"}`}
							file={file}
							revisionId={selectedRevisionId}
						/>
						<div className="rounded-lg bg-muted/30 p-3">
							<DetailRow label={t("id")} value={file.id} />
							<DetailRow label={t("admin_blob_id")} value={file.blob_id} />
							<DetailRow
								label={t("admin_policy_id")}
								value={displayValue(fileBlobSummary(file)?.policy_id)}
							/>
							<DetailRow
								label={t("admin_size")}
								value={formatBytes(file.size)}
							/>
							<DetailRow
								label={t("admin_mime_type")}
								value={displayValue(file.mime_type)}
							/>
							<DetailRow
								label={t("admin_uploaded_by")}
								value={<UserIdentity user={file.created_by} />}
							/>
							<DetailRow
								label={t("admin_storage_path")}
								value={displayValue(fileBlobSummary(file)?.storage_path)}
							/>
							<DetailRow
								label={t("admin_hash")}
								value={displayValue(fileBlobSummary(file)?.hash)}
							/>
							<DetailRow
								label={t("admin_created")}
								value={formatDateAbsoluteWithOffset(file.created_at)}
							/>
							<DetailRow
								label={t("admin_updated")}
								value={formatDateAbsoluteWithOffset(file.updated_at)}
							/>
						</div>
						<div>
							<h3 className="mb-2 text-sm font-semibold">
								{t("admin_file_versions")}
							</h3>
							{file.versions.length ? (
								<div className="space-y-2">
									{file.versions.map((version) => (
										<Link
											key={version.id}
											to={`/admin/files?fileId=${file.id}&revisionId=${version.id}`}
											aria-current={
												selectedRevisionId === version.id ? "true" : undefined
											}
											className={
												selectedRevisionId === version.id
													? "block rounded-lg bg-accent p-3 text-sm ring-1 ring-ring"
													: "block rounded-lg bg-muted/30 p-3 text-sm"
											}
										>
											<span className="font-medium underline">
												v{version.version} · {formatBytes(version.size)}
											</span>
											<div className="mt-1 break-all font-mono text-xs text-muted-foreground">
												#{version.id} · blob #{version.blob_id} ·{" "}
												{displayValue(version.blob?.hash)}
											</div>
										</Link>
									))}
								</div>
							) : (
								<div className="rounded-lg bg-muted/30 p-3 text-sm text-muted-foreground">
									{t("admin_no_file_versions")}
								</div>
							)}
						</div>
					</div>
				) : null}
			</DialogContent>
		</Dialog>
	);
}

function BlobDetailDialog(props: {
	blob: AdminFileBlobDetail | null;
	open: boolean;
	onOpenChange: (open: boolean) => void;
	onCreateMaintenanceTask: (
		action: BlobMaintenanceAction,
		blobIds: number[],
	) => Promise<void>;
	maintenanceAction: BlobMaintenanceAction | null;
}) {
	return (
		<DialogLifecycle
			open={props.open}
			target={props}
			onOpenChange={props.onOpenChange}
		>
			{({ target, active, onOpenChange }) => (
				<BlobDetailDialogContent
					{...target}
					open={active}
					onOpenChange={onOpenChange}
				/>
			)}
		</DialogLifecycle>
	);
}

function BlobDetailDialogContent({
	blob: currentBlob,
	open,
	onOpenChange,
	onCreateMaintenanceTask,
	maintenanceAction,
}: {
	blob: AdminFileBlobDetail | null;
	open: boolean;
	onOpenChange: (open: boolean) => void;
	onCreateMaintenanceTask: (
		action: BlobMaintenanceAction,
		blobIds: number[],
	) => Promise<void>;
	maintenanceAction: BlobMaintenanceAction | null;
}) {
	const blob = currentBlob;
	const { t } = useTranslation("admin");
	const isBusy = maintenanceAction !== null;
	return (
		<Dialog open={open} onOpenChange={onOpenChange}>
			<DialogContent className="max-h-[min(860px,calc(100vh-2rem))] overflow-y-auto sm:max-w-[760px]">
				<DialogHeader>
					<DialogTitle>
						{blob ? `Blob #${blob.id}` : t("admin_blob_detail")}
					</DialogTitle>
				</DialogHeader>
				{blob ? (
					<div className="space-y-4">
						<div className="flex flex-wrap gap-2">
							<Button
								variant="outline"
								size="sm"
								disabled={isBusy}
								onClick={() =>
									void onCreateMaintenanceTask("integrity_check", [blob.id])
								}
							>
								<Icon name="Shield" className="size-4" />
								{t("admin_blob_action_integrity_check")}
							</Button>
							<Button
								variant="outline"
								size="sm"
								disabled={isBusy}
								onClick={() =>
									void onCreateMaintenanceTask("ref_count_reconcile", [blob.id])
								}
							>
								<Icon name="ArrowsClockwise" className="size-4" />
								{t("admin_blob_action_reconcile_refs")}
							</Button>
							<Button
								variant={blob.health === "orphan" ? "destructive" : "outline"}
								size="sm"
								disabled={isBusy || blob.health !== "orphan"}
								onClick={() =>
									void onCreateMaintenanceTask("orphan_cleanup", [blob.id])
								}
							>
								<Icon name="Trash" className="size-4" />
								{t("admin_blob_action_cleanup_orphan")}
							</Button>
						</div>
						<div className="rounded-lg bg-muted/30 p-3">
							<DetailRow label={t("id")} value={blob.id} />
							<DetailRow label={t("admin_hash")} value={blob.hash} />
							<DetailRow label={t("admin_hash_kind")} value={blob.hash_kind} />
							<DetailRow label={t("admin_policy_id")} value={blob.policy_id} />
							<DetailRow
								label={t("admin_size")}
								value={formatBytes(blob.size)}
							/>
							<DetailRow label={t("admin_ref_count")} value={blob.ref_count} />
							<DetailRow
								label={t("admin_actual_ref_count")}
								value={blob.actual_ref_count}
							/>
							<DetailRow
								label={t("admin_file_ref_count")}
								value={blob.file_ref_count}
							/>
							<DetailRow
								label={t("admin_version_ref_count")}
								value={blob.version_ref_count}
							/>
							<DetailRow
								label={t("admin_blob_health")}
								value={t(blobHealthLabelKey(blob.health))}
							/>
							<DetailRow
								label={t("admin_uploaded_by")}
								value={
									<UserIdentityGroup
										users={blob.uploaders}
										total={blob.uploader_count}
									/>
								}
							/>
							<DetailRow
								label={t("admin_storage_path")}
								value={displayValue(blob.storage_path)}
							/>
						</div>
						{blob.files.length === 0 && blob.file_versions.length === 0 ? (
							<p>{t("content_orphan_explanation")}</p>
						) : null}
						<div className="grid gap-4 md:grid-cols-2">
							<div>
								<h3 className="mb-2 text-sm font-semibold">
									{t("admin_blob_files")}
								</h3>
								<div className="space-y-2">
									{blob.files.length ? (
										blob.files.map((file) => (
											<Link
												key={file.id}
												to={`/admin/files?fileId=${file.id}`}
												onClick={() => onOpenChange(false)}
												className="block rounded-lg bg-muted/30 p-3 text-sm transition-colors hover:bg-accent/50"
											>
												<span className="block py-1 font-medium underline">
													{file.name}
												</span>
												<p>
													{file.team_id != null
														? t("content_team_owner", { id: file.team_id })
														: t("content_personal_owner", {
																id: file.owner_user_id,
															})}{" "}
													·{" "}
													{t(
														file.deleted_at
															? "content_deleted"
															: "content_live",
													)}
												</p>
												<div className="mt-1 font-mono text-xs text-muted-foreground">
													#{file.id} · {formatBytes(file.size)}
												</div>
												<UserIdentity
													user={file.created_by}
													className="mt-2"
													size="sm"
												/>
											</Link>
										))
									) : (
										<div className="rounded-lg bg-muted/30 p-3 text-sm text-muted-foreground">
											{t("admin_no_blob_files")}
										</div>
									)}
								</div>
							</div>
							<div>
								<h3 className="mb-2 text-sm font-semibold">
									{t("admin_blob_versions")}
								</h3>
								<div className="space-y-2">
									{blob.file_versions.length ? (
										blob.file_versions.map((version) => (
											<Link
												key={version.id}
												to={`/admin/files?fileId=${version.file_id}&revisionId=${version.id}`}
												onClick={() => onOpenChange(false)}
												className="block rounded-lg bg-muted/30 p-3 text-sm transition-colors hover:bg-accent/50"
											>
												<span className="block py-1 font-medium underline">
													{version.file_name ?? `file #${version.file_id}`} · v
													{version.version}
												</span>
												<p>
													{version.team_id != null
														? t("content_team_owner", { id: version.team_id })
														: t("content_personal_owner", {
																id: version.owner_user_id ?? "—",
															})}{" "}
													·{" "}
													{t(
														version.deleted_at
															? "content_deleted"
															: "content_live",
													)}{" "}
													·{" "}
													{t(
														version.is_current
															? "content_current"
															: "content_revision",
													)}
												</p>
												<div className="mt-1 font-mono text-xs text-muted-foreground">
													#{version.id} · {formatBytes(version.size)}
												</div>
											</Link>
										))
									) : (
										<div className="rounded-lg bg-muted/30 p-3 text-sm text-muted-foreground">
											{t("admin_no_blob_versions")}
										</div>
									)}
								</div>
							</div>
						</div>
					</div>
				) : null}
			</DialogContent>
		</Dialog>
	);
}

export default function AdminFilesPage({ kind }: { kind: AdminFilesPageKind }) {
	return <AdminFilesPageContent key={kind} kind={kind} />;
}

function AdminFilesPageContent({ kind }: { kind: AdminFilesPageKind }) {
	return useAdminFilesPageContent(kind);
}

function useAdminFilesPageContent(kind: AdminFilesPageKind) {
	const { t } = useTranslation("admin");
	const isFiles = kind === "files";
	usePageTitle(isFiles ? t("admin_files") : t("admin_file_blobs"));
	const [searchParams, setSearchParams] = useSearchParams();
	const managedFileQueryDefaults = useMemo(
		() => createManagedFileQueryDefaults(kind),
		[kind],
	);
	const managedFileQuerySchema = useMemo(
		() => createManagedFileQuerySchema(kind),
		[kind],
	);
	const location = useLocation();
	const navigationType = useNavigationType();
	const { query: fileQuery, setQuery: setFileQuery } = useManagedListQueryState(
		{
			defaults: managedFileQueryDefaults,
			schema: managedFileQuerySchema,
			searchParams,
			setSearchParams,
		},
	);
	const {
		deleted,
		offset,
		ownerUserId,
		pageSize,
		policyId,
		refCountMax,
		refCountMin,
		secondaryId,
		sizeMax,
		sizeMin,
		sortBy,
		sortOrder,
		storagePath,
		teamId,
		text,
	} = fileQuery;
	const { health, hashKind, uploaderUserId } = fileQuery;
	const healthOptions = [
		{ value: "__all__", label: t("filter_health_all") },
		...BLOB_HEALTH_OPTIONS.map((value) => ({
			value,
			label: t(blobHealthLabelKey(value)),
		})),
	];
	const hashOptions = [
		{ value: "__all__", label: t("filter_hash_all") },
		...BLOB_HASH_OPTIONS.map((value) => ({
			value,
			label: t(
				value === "content_sha256"
					? "admin_hash_kind_content"
					: "admin_hash_kind_opaque",
			),
		})),
	];
	const { sizeMinUnit, sizeMaxUnit, ...listQuery } = fileQuery;
	const toDraft = () => ({
		text,
		storagePath,
		secondaryId: optionalNumberValue(secondaryId),
		teamId: optionalNumberValue(teamId),
		refCountMin: optionalNumberValue(refCountMin),
		refCountMax: optionalNumberValue(refCountMax),
		sizeMin: formatByteSizeInput(sizeMin, sizeMinUnit),
		sizeMax: formatByteSizeInput(sizeMax, sizeMaxUnit),
		sizeMinUnit,
		sizeMaxUnit,
	});
	const {
		draft,
		setDraft,
		resetDraft,
		pending: draftPending,
	} = useDebouncedQueryDraft({
		initial: toDraft(),
		sourceKey: `${kind}:${location.key}`,
		preserveUnchanged: navigationType === "REPLACE",
		normalize: (value) => {
			const min = parseByteSizeInput(value.sizeMin, value.sizeMinUnit);
			const max = parseByteSizeInput(value.sizeMax, value.sizeMaxUnit);
			if (
				min === null ||
				max === null ||
				(min != null && max != null && min > max)
			)
				return null;
			for (const key of [
				"secondaryId",
				"teamId",
				"refCountMin",
				"refCountMax",
			] as const) {
				if (value[key].trim() && parseOptionalNumber(value[key]) == null)
					return null;
			}
			return {
				...value,
				text: value.text.trim(),
				storagePath: value.storagePath.trim(),
				secondaryId: optionalNumberValue(
					parseOptionalNumber(value.secondaryId),
				),
				teamId: optionalNumberValue(parseOptionalNumber(value.teamId)),
				refCountMin: optionalNumberValue(
					parseOptionalNumber(value.refCountMin),
				),
				refCountMax: optionalNumberValue(
					parseOptionalNumber(value.refCountMax),
				),
				sizeMin: formatByteSizeInput(min, value.sizeMinUnit),
				sizeMax: formatByteSizeInput(max, value.sizeMaxUnit),
			};
		},
		onCommit: (value) =>
			setFileQuery({
				offset: 0,
				sizeMinUnit: value.sizeMinUnit,
				sizeMaxUnit: value.sizeMaxUnit,
				text: value.text,
				storagePath: value.storagePath,
				secondaryId: parseOptionalNumber(value.secondaryId),
				teamId: parseOptionalNumber(value.teamId),
				refCountMin: parseOptionalNumber(value.refCountMin),
				refCountMax: parseOptionalNumber(value.refCountMax),
				sizeMin:
					parseByteSizeInput(value.sizeMin, value.sizeMinUnit) ?? undefined,
				sizeMax:
					parseByteSizeInput(value.sizeMax, value.sizeMaxUnit) ?? undefined,
			}),
	});
	const sizeMinBytes = parseByteSizeInput(draft.sizeMin, draft.sizeMinUnit);
	const sizeMaxBytes = parseByteSizeInput(draft.sizeMax, draft.sizeMaxUnit);
	const invalidSizeRange =
		sizeMinBytes != null && sizeMaxBytes != null && sizeMinBytes > sizeMaxBytes;
	const changeSizeUnit = (
		field: "sizeMin" | "sizeMax",
		unit: ByteFilterUnit,
	) => {
		const bytes = field === "sizeMin" ? sizeMinBytes : sizeMaxBytes;
		if (bytes === null) return;
		setDraft({
			...draft,
			[`${field}Unit`]: unit,
			[field]: formatByteSizeInput(bytes, unit),
		});
	};
	const fileDetailDialog = useManagedAdminListDetailDialog<AdminFileDetail>();
	const blobDetailDialog =
		useManagedAdminListDetailDialog<AdminFileBlobDetail>();
	const selectedBlobId = !isFiles
		? parseOptionalNumber(searchParams.get("blobId"))
		: undefined;
	const { setDetail: setSelectedBlob, onOpenChange: setBlobDialogOpen } =
		blobDetailDialog;
	const [blobTargetUnavailable, setBlobTargetUnavailable] = useState(false);
	const blobSelectionGeneration = useRef(0);
	useEffect(() => {
		blobSelectionGeneration.current += 1;
		setBlobDialogOpen(false);
		setBlobTargetUnavailable(false);
		if (!selectedBlobId) return;
		let active = true;
		adminFileService
			.getBlob(selectedBlobId)
			.then((blob) => {
				if (active) setSelectedBlob(blob);
			})
			.catch((error) => {
				if (active) {
					setBlobTargetUnavailable(true);
					handleApiError(error);
				}
			});
		return () => {
			active = false;
			blobSelectionGeneration.current += 1;
		};
	}, [selectedBlobId, setSelectedBlob, setBlobDialogOpen]);
	const selectedFileId = isFiles
		? parseOptionalNumber(searchParams.get("fileId"))
		: undefined;
	const selectedRevisionId = isFiles
		? parseOptionalNumber(searchParams.get("revisionId"))
		: undefined;
	const { setDetail: setSelectedFile, onOpenChange: setFileDialogOpen } =
		fileDetailDialog;
	const [fileTargetUnavailable, setFileTargetUnavailable] = useState(false);
	useEffect(() => {
		setFileDialogOpen(false);
		setFileTargetUnavailable(false);
		if (!selectedFileId) return;
		let active = true;
		adminFileService
			.getFile(selectedFileId)
			.then((file) => {
				if (active) setSelectedFile(file);
			})
			.catch((error) => {
				if (active) {
					setFileTargetUnavailable(true);
					handleApiError(error);
				}
			});
		return () => {
			active = false;
		};
	}, [selectedFileId, setSelectedFile, setFileDialogOpen]);

	const [maintenanceAction, setMaintenanceAction] =
		useState<BlobMaintenanceAction | null>(null);
	const [fullMaintenanceAction, setFullMaintenanceAction] =
		useState<BlobMaintenanceAction | null>(null);
	const maintenanceLockRef = useRef(false);
	const setOffset = useManagedOffset(setFileQuery, normalizeOffset);
	const activeFilterCount =
		(text.trim() ? 1 : 0) +
		(policyId != null ? 1 : 0) +
		(isFiles && secondaryId != null ? 1 : 0) +
		(isFiles && ownerUserId != null ? 1 : 0) +
		(isFiles && teamId != null ? 1 : 0) +
		(!isFiles && storagePath.trim() ? 1 : 0) +
		(isFiles && deleted !== "__all__" ? 1 : 0) +
		(!isFiles && refCountMin != null ? 1 : 0) +
		(!isFiles && refCountMax != null ? 1 : 0) +
		(!isFiles && sizeMin != null ? 1 : 0) +
		(!isFiles && sizeMax != null ? 1 : 0) +
		(!isFiles && uploaderUserId != null ? 1 : 0) +
		(!isFiles && health !== "__all__" ? 1 : 0) +
		(!isFiles && hashKind !== "__all__" ? 1 : 0) +
		(draftPending ? 1 : 0);

	const {
		currentPage,
		items,
		loading,
		nextPageDisabled,
		reload,
		total,
		totalPages,
	} = useManagedAdminList<
		AdminFileInfo | AdminFileBlobInfo,
		Omit<ManagedFileQuery, "sizeMinUnit" | "sizeMaxUnit">
	>({
		deps: [isFiles],
		loadPage: (query) =>
			isFiles
				? adminFileService.listFiles({
						limit: query.pageSize,
						offset: query.offset,
						name: query.text.trim() || undefined,
						blob_id: query.secondaryId,
						owner_user_id: query.ownerUserId,
						policy_id: query.policyId,
						team_id: query.teamId,
						deleted: deletedToQuery(query.deleted),
						sort_by: query.sortBy as AdminFileSortBy,
						sort_order: query.sortOrder,
					})
				: adminFileService.listBlobs({
						limit: query.pageSize,
						offset: query.offset,
						hash: query.text.trim() || undefined,
						uploader_user_id: query.uploaderUserId,
						health: query.health === "__all__" ? undefined : query.health,
						hash_kind:
							query.hashKind === "__all__" ? undefined : query.hashKind,
						policy_id: query.policyId,
						storage_path: query.storagePath.trim() || undefined,
						ref_count_min: query.refCountMin,
						ref_count_max: query.refCountMax,
						size_min: query.sizeMin,
						size_max: query.sizeMax,
						sort_by: query.sortBy as AdminFileBlobSortBy,
						sort_order: query.sortOrder,
					}),
		query: listQuery,
		setOffset,
	});

	const handlePageSizeChange = (value: string | null) => {
		const next = parsePageSizeOption(value, PAGE_SIZE_OPTIONS);
		if (next == null) return;
		setFileQuery({ offset: 0, pageSize: next });
	};
	const handleSortChange = useCallback(
		(
			nextSortBy: AdminFileSortBy | AdminFileBlobSortBy,
			nextOrder: SortOrder,
		) => {
			setFileQuery({ offset: 0, sortBy: nextSortBy, sortOrder: nextOrder });
		},
		[setFileQuery],
	);
	const resetFilters = () => {
		resetDraft();
		setFileQuery({
			uploaderUserId: undefined,
			health: "__all__",
			hashKind: "__all__",
			sizeMinUnit: "B",
			sizeMaxUnit: "B",
			deleted: "__all__",
			offset: 0,
			ownerUserId: undefined,
			policyId: undefined,
			refCountMax: undefined,
			refCountMin: undefined,
			secondaryId: undefined,
			sizeMax: undefined,
			sizeMin: undefined,
			storagePath: "",
			teamId: undefined,
			text: "",
		});
	};
	const openFileDetail = async (id: number) => {
		setSearchParams((current) => {
			const next = new URLSearchParams(current);
			next.set("fileId", String(id));
			next.delete("revisionId");
			return next;
		});
	};
	const openBlobDetail = async (id: number) => {
		setSearchParams((current) => {
			const next = new URLSearchParams(current);
			next.set("blobId", String(id));
			return next;
		});
	};
	const createMaintenanceTask = async (
		action: BlobMaintenanceAction,
		blobIds?: number[],
	) => {
		if (maintenanceLockRef.current) return;
		const detailIdAtStart = blobDetailDialog.detail?.id;
		const selectionAtStart = blobSelectionGeneration.current;
		const isCurrentDetail = () =>
			detailIdAtStart != null &&
			detailIdAtStart === selectedBlobId &&
			blobSelectionGeneration.current === selectionAtStart;
		maintenanceLockRef.current = true;
		setMaintenanceAction(action);
		try {
			const task = await adminFileService.createBlobMaintenanceTask({
				action,
				...(blobIds ? { blob_ids: blobIds } : {}),
			});
			toast.success(t("admin_blob_maintenance_task_created", { id: task.id }));
			await reload();
			if (detailIdAtStart != null && isCurrentDetail()) {
				const detail = await adminFileService.getBlob(detailIdAtStart);
				if (isCurrentDetail()) blobDetailDialog.setDetail(detail);
			}
		} catch (error) {
			handleApiError(error);
		} finally {
			maintenanceLockRef.current = false;
			setMaintenanceAction(null);
		}
	};

	const pageSizeOptions = PAGE_SIZE_OPTIONS.map((size) => ({
		label: t("page_size_option", { count: size }),
		value: String(size),
	}));
	const selectedFullMaintenanceAction = fullMaintenanceAction;
	const isMaintenanceBusy =
		maintenanceAction !== null || maintenanceLockRef.current;
	const adminTableEmptyIcon = useMemo(
		() => <Icon name={isFiles ? "File" : "HardDrive"} className="size-10" />,
		[isFiles],
	);
	const adminTableHeaderRow = useMemo(
		() => (
			<TableHeader>
				<TableRow>
					<AdminSortableTableHead
						className="w-16"
						sortKey="id"
						sortBy={sortBy}
						sortOrder={sortOrder}
						onSortChange={handleSortChange}
					>
						{t("id")}
					</AdminSortableTableHead>
					{isFiles ? (
						<>
							<AdminSortableTableHead
								sortKey="name"
								sortBy={sortBy}
								sortOrder={sortOrder}
								onSortChange={handleSortChange}
							>
								{t("core:name")}
							</AdminSortableTableHead>
							<AdminSortableTableHead
								sortKey="size"
								sortBy={sortBy}
								sortOrder={sortOrder}
								onSortChange={handleSortChange}
							>
								{t("admin_size")}
							</AdminSortableTableHead>
							<AdminSortableTableHead
								sortKey="blob_id"
								sortBy={sortBy}
								sortOrder={sortOrder}
								onSortChange={handleSortChange}
							>
								{t("admin_blob_id")}
							</AdminSortableTableHead>
							<AdminSortableTableHead
								sortKey="policy_id"
								sortBy={sortBy}
								sortOrder={sortOrder}
								onSortChange={handleSortChange}
							>
								{t("admin_policy_id")}
							</AdminSortableTableHead>
							<TableHead>{t("admin_hash")}</TableHead>
							<TableHead>{t("admin_uploaded_by")}</TableHead>
							<TableHead>{t("core:status")}</TableHead>
							<AdminSortableTableHead
								sortKey="updated_at"
								sortBy={sortBy}
								sortOrder={sortOrder}
								onSortChange={handleSortChange}
							>
								{t("admin_updated")}
							</AdminSortableTableHead>
						</>
					) : (
						<>
							<AdminSortableTableHead
								sortKey="hash"
								sortBy={sortBy}
								sortOrder={sortOrder}
								onSortChange={handleSortChange}
							>
								{t("admin_hash")}
							</AdminSortableTableHead>
							<AdminSortableTableHead
								sortKey="size"
								sortBy={sortBy}
								sortOrder={sortOrder}
								onSortChange={handleSortChange}
							>
								{t("admin_size")}
							</AdminSortableTableHead>
							<AdminSortableTableHead
								sortKey="policy_id"
								sortBy={sortBy}
								sortOrder={sortOrder}
								onSortChange={handleSortChange}
							>
								{t("admin_policy_id")}
							</AdminSortableTableHead>
							<AdminSortableTableHead
								sortKey="ref_count"
								sortBy={sortBy}
								sortOrder={sortOrder}
								onSortChange={handleSortChange}
							>
								{t("admin_ref_count")}
							</AdminSortableTableHead>
							<TableHead>{t("admin_blob_health")}</TableHead>
							<TableHead>{t("admin_uploaded_by")}</TableHead>
							<TableHead>{t("admin_hash_kind")}</TableHead>
							<TableHead>{t("admin_storage_path")}</TableHead>
						</>
					)}
				</TableRow>
			</TableHeader>
		),
		[handleSortChange, isFiles, sortBy, sortOrder, t],
	);

	return (
		<AdminLayout>
			<AdminPageShell>
				<AdminPageHeader
					className="px-0 md:px-0"
					title={isFiles ? t("admin_files") : t("admin_file_blobs")}
					description={
						isFiles ? t("admin_files_intro") : t("admin_file_blobs_intro")
					}
					actions={
						<div className="flex flex-wrap items-center justify-end gap-2">
							{!isFiles ? (
								<DropdownMenu>
									<DropdownMenuTrigger
										render={
											<Button
												variant="outline"
												size="sm"
												disabled={isMaintenanceBusy}
											>
												<Icon name="Wrench" className="size-4" />
												{t("admin_blob_full_maintenance")}
											</Button>
										}
									/>
									<DropdownMenuContent align="end" className="w-56">
										<DropdownMenuItem
											onClick={() =>
												setFullMaintenanceAction("integrity_check")
											}
										>
											<Icon name="Shield" className="size-4" />
											{t("admin_blob_action_integrity_check")}
										</DropdownMenuItem>
										<DropdownMenuItem
											onClick={() =>
												setFullMaintenanceAction("ref_count_reconcile")
											}
										>
											<Icon name="ArrowsClockwise" className="size-4" />
											{t("admin_blob_action_reconcile_refs")}
										</DropdownMenuItem>
										<DropdownMenuItem
											variant="destructive"
											onClick={() => setFullMaintenanceAction("orphan_cleanup")}
										>
											<Icon name="Trash" className="size-4" />
											{t("admin_blob_action_cleanup_orphan")}
										</DropdownMenuItem>
									</DropdownMenuContent>
								</DropdownMenu>
							) : null}
							<Button variant="outline" size="sm" onClick={() => void reload()}>
								<Icon name="ArrowClockwise" className="size-4" />
								{t("core:refresh")}
							</Button>
						</div>
					}
					toolbar={
						<AdminFilterToolbar
							activeFilterCount={activeFilterCount}
							inline
							onResetFilters={resetFilters}
						>
							<div className="relative min-w-[220px] flex-1 md:max-w-xs">
								<Icon
									name="MagnifyingGlass"
									className="pointer-events-none absolute top-1/2 left-3 size-4 -translate-y-1/2 text-muted-foreground"
								/>
								<Input
									value={draft.text}
									onChange={(event) => {
										setDraft({ ...draft, text: event.target.value });
									}}
									placeholder={
										isFiles
											? t("admin_file_name_filter")
											: t("admin_blob_hash_filter")
									}
									className={`${ADMIN_CONTROL_HEIGHT_CLASS} pl-9`}
								/>
							</div>
							<AdminSearchFilter
								kind="policies"
								value={policyId}
								label={t("filter_policy")}
								onChange={(policyId) => setFileQuery({ offset: 0, policyId })}
							/>
							<AdminSearchFilter
								kind="users"
								value={isFiles ? ownerUserId : uploaderUserId}
								label={t(isFiles ? "filter_owner" : "filter_uploader")}
								onChange={(userId) =>
									setFileQuery({
										offset: 0,
										...(isFiles
											? { ownerUserId: userId }
											: { uploaderUserId: userId }),
									})
								}
							/>

							{isFiles ? (
								<>
									<Input
										value={draft.secondaryId}
										onChange={(event) => {
											setDraft({ ...draft, secondaryId: event.target.value });
										}}
										placeholder={t("admin_blob_id")}
										className={`${ADMIN_CONTROL_HEIGHT_CLASS} w-32`}
									/>
									<Input
										value={draft.teamId}
										onChange={(event) => {
											setDraft({ ...draft, teamId: event.target.value });
										}}
										placeholder={t("admin_team_id")}
										className={`${ADMIN_CONTROL_HEIGHT_CLASS} w-32`}
									/>
									<Select
										value={deleted}
										onValueChange={(value) => {
											setFileQuery({
												deleted: parseDeletedFilter(value),
												offset: 0,
											});
										}}
									>
										<SelectTrigger width="compact">
											<SelectValue>
												{t(deletedFilterLabelKey(deleted))}
											</SelectValue>
										</SelectTrigger>
										<SelectContent>
											<SelectItem value="__all__">
												{t(deletedFilterLabelKey("__all__"))}
											</SelectItem>
											<SelectItem value="live">
												{t(deletedFilterLabelKey("live"))}
											</SelectItem>
											<SelectItem value="deleted">
												{t(deletedFilterLabelKey("deleted"))}
											</SelectItem>
										</SelectContent>
									</Select>
								</>
							) : (
								<>
									<Input
										value={draft.storagePath}
										onChange={(event) => {
											setDraft({ ...draft, storagePath: event.target.value });
										}}
										placeholder={t("admin_storage_path")}
										className={`${ADMIN_CONTROL_HEIGHT_CLASS} w-48`}
									/>
									<Input
										value={draft.refCountMin}
										onChange={(event) => {
											setDraft({ ...draft, refCountMin: event.target.value });
										}}
										placeholder={t("admin_ref_count_min")}
										className={`${ADMIN_CONTROL_HEIGHT_CLASS} w-36`}
									/>
									<Input
										value={draft.refCountMax}
										onChange={(event) => {
											setDraft({ ...draft, refCountMax: event.target.value });
										}}
										placeholder={t("admin_ref_count_max")}
										className={`${ADMIN_CONTROL_HEIGHT_CLASS} w-36`}
									/>
									{(["sizeMin", "sizeMax"] as const).map((field) => (
										<AdminNumberUnitInput
											key={field}
											className="w-full sm:w-56"
											inline
											value={draft[field]}
											unit={draft[`${field}Unit`]}
											units={BYTE_FILTER_UNITS}
											step="any"
											placeholder={t(
												field === "sizeMin"
													? "admin_size_min"
													: "admin_size_max",
											)}
											unitAriaLabel={t(
												field === "sizeMin"
													? "filter_size_min_unit"
													: "filter_size_max_unit",
											)}
											onValueChange={(value) =>
												setDraft({ ...draft, [field]: value })
											}
											onUnitChange={(unit) => changeSizeUnit(field, unit)}
											errorMessage={
												parseByteSizeInput(
													draft[field],
													draft[`${field}Unit`],
												) === null || invalidSizeRange
													? t("filter_size_invalid")
													: null
											}
										/>
									))}
									<Select
										items={healthOptions}
										value={health}
										onValueChange={(value) =>
											setFileQuery({
												offset: 0,
												health: BLOB_HEALTH_OPTIONS.includes(
													value as AdminFileBlobHealth,
												)
													? (value as AdminFileBlobHealth)
													: "__all__",
											})
										}
									>
										<SelectTrigger
											width="fit"
											aria-label={t("admin_blob_health")}
										>
											<SelectValue />
										</SelectTrigger>
										<SelectContent>
											{healthOptions.map(({ value, label }) => (
												<SelectItem key={value} value={value}>
													{label}
												</SelectItem>
											))}
										</SelectContent>
									</Select>
									<Select
										items={hashOptions}
										value={hashKind}
										onValueChange={(value) =>
											setFileQuery({
												offset: 0,
												hashKind: BLOB_HASH_OPTIONS.includes(
													value as AdminFileBlobHashKind,
												)
													? (value as AdminFileBlobHashKind)
													: "__all__",
											})
										}
									>
										<SelectTrigger
											width="fit"
											aria-label={t("admin_hash_kind")}
										>
											<SelectValue />
										</SelectTrigger>
										<SelectContent>
											{hashOptions.map(({ value, label }) => (
												<SelectItem key={value} value={value}>
													{label}
												</SelectItem>
											))}
										</SelectContent>
									</Select>
								</>
							)}
						</AdminFilterToolbar>
					}
				/>
				<AdminTableList
					frameless
					loading={loading}
					items={items}
					columns={9}
					rows={6}
					emptyIcon={adminTableEmptyIcon}
					emptyTitle={isFiles ? t("admin_no_files") : t("admin_no_blobs")}
					emptyDescription={
						isFiles ? t("admin_no_files_desc") : t("admin_no_blobs_desc")
					}
					headerRow={adminTableHeaderRow}
					renderRow={(item) =>
						isFiles ? (
							<FileRow
								key={item.id}
								file={item as AdminFileInfo}
								onOpenDetail={openFileDetail}
							/>
						) : (
							<BlobRow
								key={item.id}
								blob={item as AdminFileBlobInfo}
								onOpenDetail={openBlobDetail}
							/>
						)
					}
				/>
				<AdminOffsetPagination
					currentPage={currentPage}
					nextDisabled={nextPageDisabled}
					onNext={() => setOffset((current) => current + pageSize)}
					onPageSizeChange={handlePageSizeChange}
					onPrevious={() => setOffset(Math.max(0, offset - pageSize))}
					pageSize={String(pageSize)}
					pageSizeOptions={pageSizeOptions}
					prevDisabled={offset === 0}
					total={total}
					totalPages={totalPages}
				/>
			</AdminPageShell>
			{blobTargetUnavailable ? (
				<p role="alert">{t("content_blob_unavailable")}</p>
			) : null}
			{fileTargetUnavailable ? (
				<p role="alert">{t("content_file_unavailable")}</p>
			) : null}
			<FileDetailDialog
				file={fileDetailDialog.detail}
				revisionId={selectedRevisionId}
				open={fileDetailDialog.open}
				onOpenChange={(open) => {
					if (!open)
						setSearchParams((current) => {
							const next = new URLSearchParams(current);
							next.delete("fileId");
							next.delete("revisionId");
							return next;
						});
					setFileDialogOpen(open);
				}}
			/>
			<BlobDetailDialog
				blob={blobDetailDialog.detail}
				open={blobDetailDialog.open}
				maintenanceAction={
					isMaintenanceBusy ? (maintenanceAction ?? "integrity_check") : null
				}
				onOpenChange={(open) => {
					if (!open)
						setSearchParams((current) => {
							const next = new URLSearchParams(current);
							next.delete("blobId");
							return next;
						});
					setBlobDialogOpen(open);
				}}
				onCreateMaintenanceTask={createMaintenanceTask}
			/>
			<ConfirmDialog
				open={selectedFullMaintenanceAction !== null}
				onOpenChange={(open) => {
					if (!open) setFullMaintenanceAction(null);
				}}
				title={t("admin_blob_full_maintenance_confirm_title")}
				description={
					selectedFullMaintenanceAction
						? t(
								`admin_blob_full_maintenance_confirm_desc_${selectedFullMaintenanceAction}`,
							)
						: undefined
				}
				confirmLabel={t("admin_blob_full_maintenance_confirm")}
				variant={
					selectedFullMaintenanceAction === "orphan_cleanup"
						? "destructive"
						: "default"
				}
				onConfirm={() => {
					if (!selectedFullMaintenanceAction) return;
					void createMaintenanceTask(selectedFullMaintenanceAction);
					setFullMaintenanceAction(null);
				}}
			/>
		</AdminLayout>
	);
}

function FileRow({
	file,
	onOpenDetail,
}: {
	file: AdminFileInfo;
	onOpenDetail: (id: number) => void;
}) {
	const { t } = useTranslation("admin");
	const blob = file.blob;
	const handleKeyDown = (event: React.KeyboardEvent<HTMLTableRowElement>) => {
		if (event.key === "Enter" || event.key === " ") {
			event.preventDefault();
			onOpenDetail(file.id);
		}
	};

	return (
		<TableRow
			className={ADMIN_INTERACTIVE_TABLE_ROW_CLASS}
			onClick={() => void onOpenDetail(file.id)}
			onKeyDown={handleKeyDown}
			tabIndex={0}
		>
			<TableCell>
				<span className={ADMIN_TABLE_MONO_TEXT_CLASS}>{file.id}</span>
			</TableCell>
			<TableCell>
				<div className={ADMIN_TABLE_STACKED_CELL_CLASS}>
					<span className="truncate font-medium text-foreground">
						{file.name}
					</span>
					<span className="truncate text-xs text-muted-foreground">
						{file.mime_type}
					</span>
				</div>
			</TableCell>
			<TableCell>{formatBytes(file.size)}</TableCell>
			<TableCell>
				<span className={ADMIN_TABLE_MONO_TEXT_CLASS}>{file.blob_id}</span>
			</TableCell>
			<TableCell>
				<span className={ADMIN_TABLE_MONO_TEXT_CLASS}>
					{displayValue(blob?.policy_id)}
				</span>
			</TableCell>
			<TableCell>
				<div className={ADMIN_TABLE_TEXT_CELL_CLASS}>
					<span className="truncate font-mono text-xs text-muted-foreground">
						{hashPreview(blob?.hash)}
					</span>
				</div>
			</TableCell>
			<TableCell>
				<UserIdentity user={file.created_by} />
			</TableCell>
			<TableCell>
				<div className={ADMIN_TABLE_BADGE_CELL_CLASS}>
					<Badge variant="outline">
						{t(deletedFilterLabelKey(file.deleted_at ? "deleted" : "live"))}
					</Badge>
				</div>
			</TableCell>
			<TableCell>
				<span className="whitespace-nowrap text-xs text-muted-foreground">
					{formatDateAbsoluteWithOffset(file.updated_at)}
				</span>
			</TableCell>
		</TableRow>
	);
}

function BlobRow({
	blob,
	onOpenDetail,
}: {
	blob: AdminFileBlobInfo;
	onOpenDetail: (id: number) => void;
}) {
	const { t } = useTranslation("admin");
	const handleKeyDown = (event: React.KeyboardEvent<HTMLTableRowElement>) => {
		if (event.key === "Enter" || event.key === " ") {
			event.preventDefault();
			onOpenDetail(blob.id);
		}
	};

	return (
		<TableRow
			className={ADMIN_INTERACTIVE_TABLE_ROW_CLASS}
			onClick={() => void onOpenDetail(blob.id)}
			onKeyDown={handleKeyDown}
			tabIndex={0}
		>
			<TableCell>
				<span className={ADMIN_TABLE_MONO_TEXT_CLASS}>{blob.id}</span>
			</TableCell>
			<TableCell>
				<div className={ADMIN_TABLE_TEXT_CELL_CLASS}>
					<span className="truncate font-mono text-xs text-muted-foreground">
						{hashPreview(blob.hash)}
					</span>
				</div>
			</TableCell>
			<TableCell>{formatBytes(blob.size)}</TableCell>
			<TableCell>
				<span className={ADMIN_TABLE_MONO_TEXT_CLASS}>{blob.policy_id}</span>
			</TableCell>
			<TableCell>
				<div className={ADMIN_TABLE_STACKED_CELL_CLASS}>
					<span className={ADMIN_TABLE_MONO_TEXT_CLASS}>{blob.ref_count}</span>
					<span className="text-xs text-muted-foreground">
						{t("admin_blob_actual_ref_count_short", {
							count: blob.actual_ref_count,
						})}
					</span>
				</div>
			</TableCell>
			<TableCell>
				<div className={ADMIN_TABLE_BADGE_CELL_CLASS}>
					<Badge variant={blobHealthVariant(blob.health)}>
						{t(blobHealthLabelKey(blob.health))}
					</Badge>
				</div>
			</TableCell>
			<TableCell>
				<UserIdentityGroup users={blob.uploaders} total={blob.uploader_count} />
			</TableCell>
			<TableCell>
				<div className={ADMIN_TABLE_BADGE_CELL_CLASS}>
					<Badge variant="outline">
						{blob.hash_kind === "content_sha256"
							? t("admin_hash_kind_content")
							: t("admin_hash_kind_opaque")}
					</Badge>
				</div>
			</TableCell>
			<TableCell>
				<div className={ADMIN_TABLE_TEXT_CELL_CLASS}>
					<span className="truncate text-xs text-muted-foreground">
						{displayValue(blob.storage_path)}
					</span>
				</div>
			</TableCell>
		</TableRow>
	);
}
