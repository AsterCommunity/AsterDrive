import type { SetStateAction } from "react";
import { useEffect, useRef, useState } from "react";
import { useTranslation } from "react-i18next";
import { useSearchParams } from "react-router-dom";
import { AdminOffsetPagination } from "@/components/admin/AdminOffsetPagination";
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
import { useApiList } from "@/hooks/useApiList";
import { usePageTitle } from "@/hooks/usePageTitle";
import { ADMIN_CONTROL_HEIGHT_CLASS } from "@/lib/constants";
import { formatBytes, formatDateAbsoluteWithOffset } from "@/lib/format";
import {
	buildOffsetPaginationSearchParams,
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
	AdminFileBlobHealth,
	AdminFileBlobInfo,
	AdminFileDetail,
	AdminFileInfo,
} from "@/types/api";

type AdminFilesPageKind = "files" | "blobs";
type DeletedFilter = "__all__" | "live" | "deleted";

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
const MANAGED_QUERY_KEYS = [
	"blobId",
	"deleted",
	"hash",
	"name",
	"offset",
	"ownerUserId",
	"pageSize",
	"policyId",
	"refCountMax",
	"refCountMin",
	"sizeMax",
	"sizeMin",
	"sortBy",
	"sortOrder",
	"storagePath",
	"teamId",
] as const;

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

function buildManagedSearchParams({
	kind,
	offset,
	pageSize,
	sortBy,
	sortOrder,
	text,
	policyId,
	secondaryId,
	ownerUserId,
	teamId,
	deleted,
	refCountMin,
	refCountMax,
	sizeMin,
	sizeMax,
}: {
	kind: AdminFilesPageKind;
	offset: number;
	pageSize: (typeof PAGE_SIZE_OPTIONS)[number];
	sortBy: AdminFileSortBy | AdminFileBlobSortBy;
	sortOrder: SortOrder;
	text: string;
	policyId?: number;
	secondaryId?: number;
	ownerUserId?: number;
	teamId?: number;
	deleted?: DeletedFilter;
	refCountMin?: number;
	refCountMax?: number;
	sizeMin?: number;
	sizeMax?: number;
}) {
	const isFiles = kind === "files";
	return buildOffsetPaginationSearchParams({
		offset,
		pageSize,
		defaultPageSize: DEFAULT_PAGE_SIZE,
		extraParams: {
			[isFiles ? "name" : "hash"]: text.trim() || undefined,
			[isFiles ? "blobId" : "storagePath"]: isFiles
				? secondaryId
				: text.trim()
					? undefined
					: undefined,
			ownerUserId: isFiles ? ownerUserId : undefined,
			teamId: isFiles ? teamId : undefined,
			policyId,
			deleted:
				isFiles && deleted && deleted !== "__all__" ? deleted : undefined,
			refCountMin: !isFiles ? refCountMin : undefined,
			refCountMax: !isFiles ? refCountMax : undefined,
			sizeMin: !isFiles ? sizeMin : undefined,
			sizeMax: !isFiles ? sizeMax : undefined,
			sortBy:
				sortBy !== (isFiles ? DEFAULT_FILE_SORT_BY : DEFAULT_BLOB_SORT_BY)
					? sortBy
					: undefined,
			sortOrder: sortOrder !== DEFAULT_SORT_ORDER ? sortOrder : undefined,
		},
	});
}

function mergeManagedSearchParams(
	searchParams: URLSearchParams,
	managedSearchParams: URLSearchParams,
) {
	const merged = new URLSearchParams(searchParams);
	for (const key of MANAGED_QUERY_KEYS) {
		merged.delete(key);
	}
	for (const [key, value] of managedSearchParams.entries()) {
		merged.set(key, value);
	}
	return merged;
}

function DetailRow({
	label,
	value,
}: {
	label: string;
	value: string | number;
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

function FileDetailDialog({
	file,
	open,
	onOpenChange,
}: {
	file: AdminFileDetail | null;
	open: boolean;
	onOpenChange: (open: boolean) => void;
}) {
	const { t } = useTranslation("admin");
	return (
		<Dialog open={open} onOpenChange={onOpenChange}>
			<DialogContent className="max-h-[min(860px,calc(100vh-2rem))] overflow-y-auto sm:max-w-[760px]">
				<DialogHeader>
					<DialogTitle>{file?.name ?? t("admin_file_detail")}</DialogTitle>
				</DialogHeader>
				{file ? (
					<div className="space-y-4">
						<div className="rounded-lg border border-border/60 p-3">
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
							<div className="border-b border-border/50 py-2 text-sm last:border-b-0">
								<div className="mb-2 text-muted-foreground">
									{t("admin_uploaded_by")}
								</div>
								<UserIdentity user={file.created_by} />
							</div>
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
										<div
											key={version.id}
											className="rounded-lg border border-border/60 p-3 text-sm"
										>
											<div className="font-medium">
												v{version.version} · {formatBytes(version.size)}
											</div>
											<div className="mt-1 break-all font-mono text-xs text-muted-foreground">
												#{version.id} · blob #{version.blob_id} ·{" "}
												{displayValue(version.blob?.hash)}
											</div>
										</div>
									))}
								</div>
							) : (
								<div className="rounded-lg border border-border/60 p-3 text-sm text-muted-foreground">
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

function BlobDetailDialog({
	blob,
	open,
	onOpenChange,
}: {
	blob: AdminFileBlobDetail | null;
	open: boolean;
	onOpenChange: (open: boolean) => void;
}) {
	const { t } = useTranslation("admin");
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
						<div className="rounded-lg border border-border/60 p-3">
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
							<div className="border-b border-border/50 py-2 text-sm last:border-b-0">
								<div className="mb-2 text-muted-foreground">
									{t("admin_uploaded_by")}
								</div>
								<UserIdentityGroup
									users={blob.uploaders}
									total={blob.uploader_count}
								/>
							</div>
							<DetailRow
								label={t("admin_storage_path")}
								value={blob.storage_path}
							/>
						</div>
						<div className="grid gap-4 md:grid-cols-2">
							<div>
								<h3 className="mb-2 text-sm font-semibold">
									{t("admin_blob_files")}
								</h3>
								<div className="space-y-2">
									{blob.files.length ? (
										blob.files.map((file) => (
											<div
												key={file.id}
												className="rounded-lg border border-border/60 p-3 text-sm"
											>
												<div className="truncate font-medium">{file.name}</div>
												<div className="mt-1 font-mono text-xs text-muted-foreground">
													#{file.id} · {formatBytes(file.size)}
												</div>
												<UserIdentity
													user={file.created_by}
													className="mt-2"
													size="sm"
												/>
											</div>
										))
									) : (
										<div className="rounded-lg border border-border/60 p-3 text-sm text-muted-foreground">
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
											<div
												key={version.id}
												className="rounded-lg border border-border/60 p-3 text-sm"
											>
												<div className="font-medium">
													file #{version.file_id} · v{version.version}
												</div>
												<div className="mt-1 font-mono text-xs text-muted-foreground">
													#{version.id} · {formatBytes(version.size)}
												</div>
											</div>
										))
									) : (
										<div className="rounded-lg border border-border/60 p-3 text-sm text-muted-foreground">
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
	return <AdminFilesPageView key={kind} kind={kind} />;
}

function AdminFilesPageView({ kind }: { kind: AdminFilesPageKind }) {
	const { t } = useTranslation("admin");
	const isFiles = kind === "files";
	usePageTitle(isFiles ? t("admin_files") : t("admin_file_blobs"));
	const [searchParams, setSearchParams] = useSearchParams();
	const [offset, setOffsetState] = useState(
		normalizeOffset(parseOffsetSearchParam(searchParams.get("offset"))),
	);
	const [pageSize, setPageSize] = useState<(typeof PAGE_SIZE_OPTIONS)[number]>(
		parsePageSizeSearchParam(
			searchParams.get("pageSize"),
			PAGE_SIZE_OPTIONS,
			DEFAULT_PAGE_SIZE,
		),
	);
	const [text, setText] = useState(
		searchParams.get(isFiles ? "name" : "hash") ?? "",
	);
	const [policyId, setPolicyId] = useState<number | undefined>(
		parseOptionalNumber(searchParams.get("policyId")),
	);
	const [secondaryId, setSecondaryId] = useState<number | undefined>(
		parseOptionalNumber(searchParams.get("blobId")),
	);
	const [ownerUserId, setOwnerUserId] = useState<number | undefined>(
		parseOptionalNumber(searchParams.get("ownerUserId")),
	);
	const [teamId, setTeamId] = useState<number | undefined>(
		parseOptionalNumber(searchParams.get("teamId")),
	);
	const [storagePath, setStoragePath] = useState(
		searchParams.get("storagePath") ?? "",
	);
	const [deleted, setDeleted] = useState<DeletedFilter>(
		parseDeletedFilter(searchParams.get("deleted")),
	);
	const [refCountMin, setRefCountMin] = useState<number | undefined>(
		parseOptionalNumber(searchParams.get("refCountMin")),
	);
	const [refCountMax, setRefCountMax] = useState<number | undefined>(
		parseOptionalNumber(searchParams.get("refCountMax")),
	);
	const [sizeMin, setSizeMin] = useState<number | undefined>(
		parseOptionalNumber(searchParams.get("sizeMin")),
	);
	const [sizeMax, setSizeMax] = useState<number | undefined>(
		parseOptionalNumber(searchParams.get("sizeMax")),
	);
	const [sortBy, setSortBy] = useState<AdminFileSortBy | AdminFileBlobSortBy>(
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
	);
	const [sortOrder, setSortOrder] = useState<SortOrder>(
		parseSortOrderSearchParam(
			searchParams.get("sortOrder"),
			DEFAULT_SORT_ORDER,
		),
	);
	const [fileDetail, setFileDetail] = useState<AdminFileDetail | null>(null);
	const [blobDetail, setBlobDetail] = useState<AdminFileBlobDetail | null>(
		null,
	);
	const lastWrittenSearchRef = useRef<string | null>(null);
	const setOffset = (value: SetStateAction<number>) => {
		setOffsetState((current) =>
			normalizeOffset(typeof value === "function" ? value(current) : value),
		);
	};
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
		(!isFiles && sizeMax != null ? 1 : 0);

	useEffect(() => {
		const nextManagedSearchParams = buildManagedSearchParams({
			kind,
			offset,
			pageSize,
			sortBy,
			sortOrder,
			text,
			policyId,
			secondaryId,
			ownerUserId,
			teamId,
			deleted,
			refCountMin,
			refCountMax,
			sizeMin,
			sizeMax,
		});
		if (!isFiles && storagePath.trim()) {
			nextManagedSearchParams.set("storagePath", storagePath.trim());
		}
		const nextSearch = nextManagedSearchParams.toString();
		if (nextSearch === lastWrittenSearchRef.current) return;
		lastWrittenSearchRef.current = nextSearch;
		setSearchParams(
			mergeManagedSearchParams(searchParams, nextManagedSearchParams),
			{
				replace: true,
			},
		);
	}, [
		deleted,
		isFiles,
		kind,
		offset,
		ownerUserId,
		pageSize,
		policyId,
		refCountMax,
		refCountMin,
		searchParams,
		secondaryId,
		setSearchParams,
		sizeMax,
		sizeMin,
		sortBy,
		sortOrder,
		storagePath,
		teamId,
		text,
	]);

	const { items, loading, reload, total } = useApiList<
		AdminFileInfo | AdminFileBlobInfo
	>(
		() =>
			isFiles
				? adminFileService.listFiles({
						limit: pageSize,
						offset,
						name: text.trim() || undefined,
						blob_id: secondaryId,
						owner_user_id: ownerUserId,
						policy_id: policyId,
						team_id: teamId,
						deleted: deletedToQuery(deleted),
						sort_by: sortBy as AdminFileSortBy,
						sort_order: sortOrder,
					})
				: adminFileService.listBlobs({
						limit: pageSize,
						offset,
						hash: text.trim() || undefined,
						policy_id: policyId,
						storage_path: storagePath.trim() || undefined,
						ref_count_min: refCountMin,
						ref_count_max: refCountMax,
						size_min: sizeMin,
						size_max: sizeMax,
						sort_by: sortBy as AdminFileBlobSortBy,
						sort_order: sortOrder,
					}),
		[
			deleted,
			isFiles,
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
		],
	);
	const totalPages = Math.max(1, Math.ceil(total / pageSize));
	const currentPage = Math.floor(offset / pageSize) + 1;

	const handlePageSizeChange = (value: string | null) => {
		const next = parsePageSizeOption(value, PAGE_SIZE_OPTIONS);
		if (next == null) return;
		setPageSize(next);
		setOffset(0);
	};
	const handleSortChange = (
		nextSortBy: AdminFileSortBy | AdminFileBlobSortBy,
		nextOrder: SortOrder,
	) => {
		setSortBy(nextSortBy);
		setSortOrder(nextOrder);
		setOffset(0);
	};
	const resetFilters = () => {
		setText("");
		setPolicyId(undefined);
		setSecondaryId(undefined);
		setOwnerUserId(undefined);
		setTeamId(undefined);
		setStoragePath("");
		setDeleted("__all__");
		setRefCountMin(undefined);
		setRefCountMax(undefined);
		setSizeMin(undefined);
		setSizeMax(undefined);
		setOffset(0);
	};
	const openFileDetail = async (id: number) => {
		try {
			setFileDetail(await adminFileService.getFile(id));
		} catch (error) {
			handleApiError(error);
		}
	};
	const openBlobDetail = async (id: number) => {
		try {
			setBlobDetail(await adminFileService.getBlob(id));
		} catch (error) {
			handleApiError(error);
		}
	};

	const pageSizeOptions = PAGE_SIZE_OPTIONS.map((size) => ({
		label: t("page_size_option", { count: size }),
		value: String(size),
	}));

	return (
		<AdminLayout>
			<AdminPageShell>
				<AdminPageHeader
					title={isFiles ? t("admin_files") : t("admin_file_blobs")}
					description={
						isFiles ? t("admin_files_intro") : t("admin_file_blobs_intro")
					}
					actions={
						<Button variant="outline" size="sm" onClick={() => void reload()}>
							<Icon name="ArrowClockwise" className="size-4" />
							{t("core:refresh")}
						</Button>
					}
					toolbar={
						<>
							<div className="relative min-w-[220px] flex-1 md:max-w-xs">
								<Icon
									name="MagnifyingGlass"
									className="pointer-events-none absolute top-1/2 left-3 size-4 -translate-y-1/2 text-muted-foreground"
								/>
								<Input
									value={text}
									onChange={(event) => {
										setText(event.target.value);
										setOffset(0);
									}}
									placeholder={
										isFiles
											? t("admin_file_name_filter")
											: t("admin_blob_hash_filter")
									}
									className={`${ADMIN_CONTROL_HEIGHT_CLASS} pl-9`}
								/>
							</div>
							<Input
								value={optionalNumberValue(policyId)}
								onChange={(event) => {
									setPolicyId(parseOptionalNumber(event.target.value));
									setOffset(0);
								}}
								placeholder={t("admin_policy_id")}
								className={`${ADMIN_CONTROL_HEIGHT_CLASS} w-32`}
							/>
							{isFiles ? (
								<>
									<Input
										value={optionalNumberValue(secondaryId)}
										onChange={(event) => {
											setSecondaryId(parseOptionalNumber(event.target.value));
											setOffset(0);
										}}
										placeholder={t("admin_blob_id")}
										className={`${ADMIN_CONTROL_HEIGHT_CLASS} w-32`}
									/>
									<Input
										value={optionalNumberValue(ownerUserId)}
										onChange={(event) => {
											setOwnerUserId(parseOptionalNumber(event.target.value));
											setOffset(0);
										}}
										placeholder={t("admin_owner_user_id")}
										className={`${ADMIN_CONTROL_HEIGHT_CLASS} w-36`}
									/>
									<Input
										value={optionalNumberValue(teamId)}
										onChange={(event) => {
											setTeamId(parseOptionalNumber(event.target.value));
											setOffset(0);
										}}
										placeholder={t("admin_team_id")}
										className={`${ADMIN_CONTROL_HEIGHT_CLASS} w-32`}
									/>
									<Select
										value={deleted}
										onValueChange={(value) => {
											setDeleted(parseDeletedFilter(value));
											setOffset(0);
										}}
									>
										<SelectTrigger width="compact">
											<SelectValue />
										</SelectTrigger>
										<SelectContent>
											<SelectItem value="__all__">
												{t("admin_deleted_all")}
											</SelectItem>
											<SelectItem value="live">
												{t("admin_deleted_live")}
											</SelectItem>
											<SelectItem value="deleted">
												{t("admin_deleted_deleted")}
											</SelectItem>
										</SelectContent>
									</Select>
								</>
							) : (
								<>
									<Input
										value={storagePath}
										onChange={(event) => {
											setStoragePath(event.target.value);
											setOffset(0);
										}}
										placeholder={t("admin_storage_path")}
										className={`${ADMIN_CONTROL_HEIGHT_CLASS} w-48`}
									/>
									<Input
										value={optionalNumberValue(refCountMin)}
										onChange={(event) => {
											setRefCountMin(parseOptionalNumber(event.target.value));
											setOffset(0);
										}}
										placeholder={t("admin_ref_count_min")}
										className={`${ADMIN_CONTROL_HEIGHT_CLASS} w-36`}
									/>
									<Input
										value={optionalNumberValue(refCountMax)}
										onChange={(event) => {
											setRefCountMax(parseOptionalNumber(event.target.value));
											setOffset(0);
										}}
										placeholder={t("admin_ref_count_max")}
										className={`${ADMIN_CONTROL_HEIGHT_CLASS} w-36`}
									/>
									<Input
										value={optionalNumberValue(sizeMin)}
										onChange={(event) => {
											setSizeMin(parseOptionalNumber(event.target.value));
											setOffset(0);
										}}
										placeholder={t("admin_size_min")}
										className={`${ADMIN_CONTROL_HEIGHT_CLASS} w-32`}
									/>
									<Input
										value={optionalNumberValue(sizeMax)}
										onChange={(event) => {
											setSizeMax(parseOptionalNumber(event.target.value));
											setOffset(0);
										}}
										placeholder={t("admin_size_max")}
										className={`${ADMIN_CONTROL_HEIGHT_CLASS} w-32`}
									/>
								</>
							)}
							{activeFilterCount > 0 ? (
								<Button
									variant="ghost"
									size="sm"
									className={ADMIN_CONTROL_HEIGHT_CLASS}
									onClick={resetFilters}
								>
									{t("clear_filters")}
								</Button>
							) : null}
						</>
					}
				/>
				<AdminTableList
					loading={loading}
					items={items}
					columns={isFiles ? 9 : 9}
					rows={6}
					emptyIcon={
						<Icon name={isFiles ? "File" : "HardDrive"} className="size-10" />
					}
					emptyTitle={isFiles ? t("admin_no_files") : t("admin_no_blobs")}
					emptyDescription={
						isFiles ? t("admin_no_files_desc") : t("admin_no_blobs_desc")
					}
					headerRow={
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
					}
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
					nextDisabled={offset + pageSize >= total}
					onNext={() => setOffset(offset + pageSize)}
					onPageSizeChange={handlePageSizeChange}
					onPrevious={() => setOffset(Math.max(0, offset - pageSize))}
					pageSize={String(pageSize)}
					pageSizeOptions={pageSizeOptions}
					prevDisabled={offset === 0}
					total={total}
					totalPages={totalPages}
				/>
			</AdminPageShell>
			<FileDetailDialog
				file={fileDetail}
				open={fileDetail !== null}
				onOpenChange={(open) => {
					if (!open) setFileDetail(null);
				}}
			/>
			<BlobDetailDialog
				blob={blobDetail}
				open={blobDetail !== null}
				onOpenChange={(open) => {
					if (!open) setBlobDetail(null);
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
						{file.deleted_at
							? t("admin_deleted_deleted")
							: t("admin_deleted_live")}
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
						{blob.storage_path}
					</span>
				</div>
			</TableCell>
		</TableRow>
	);
}
