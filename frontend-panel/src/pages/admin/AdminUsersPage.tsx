import type { FormEvent, SetStateAction } from "react";
import { useEffect, useMemo, useRef, useState } from "react";
import { useTranslation } from "react-i18next";
import { useNavigate, useSearchParams } from "react-router-dom";
import { toast } from "sonner";
import { AdminOffsetPagination } from "@/components/admin/AdminOffsetPagination";
import { CreateUserDialog } from "@/components/admin/admin-users-page/CreateUserDialog";
import { GeneratedPasswordDialog } from "@/components/admin/admin-users-page/GeneratedPasswordDialog";
import { InviteUserDialog } from "@/components/admin/admin-users-page/InviteUserDialog";
import {
	UsersTableHeader,
	UsersTableRow,
} from "@/components/admin/admin-users-page/UsersTable";
import { UsersToolbar } from "@/components/admin/admin-users-page/UsersToolbar";
import { UserDetailDialog } from "@/components/admin/UserDetailDialog";
import { AdminTableList } from "@/components/common/AdminTableList";
import { ConfirmDialog } from "@/components/common/ConfirmDialog";
import { AdminLayout } from "@/components/layout/AdminLayout";
import { AdminPageHeader } from "@/components/layout/AdminPageHeader";
import { AdminPageShell } from "@/components/layout/AdminPageShell";
import { Button } from "@/components/ui/button";
import { Icon } from "@/components/ui/icon";
import { handleApiError } from "@/hooks/useApiError";
import { useApiList } from "@/hooks/useApiList";
import { useConfirmDialog } from "@/hooks/useConfirmDialog";
import { usePageTitle } from "@/hooks/usePageTitle";
import { usePendingAction } from "@/hooks/usePendingAction";
import { usePendingId } from "@/hooks/usePendingId";
import { loadAdminPolicyGroupLookup } from "@/lib/adminPolicyGroupLookup";
import { writeTextToClipboard } from "@/lib/clipboard";
import { ADMIN_CONTROL_HEIGHT_CLASS } from "@/lib/constants";
import { runWhenIdle } from "@/lib/idleTask";
import {
	buildOffsetPaginationSearchParams,
	parseOffsetSearchParam,
	parsePageSizeOption,
	parsePageSizeSearchParam,
	parseSortOrderSearchParam,
	parseSortSearchParam,
	type SortOrder,
} from "@/lib/pagination";
import { emailSchema, passwordSchema, usernameSchema } from "@/lib/validation";
import { adminUserService } from "@/services/adminService";
import type { AdminUserSortBy } from "@/types/adminSort";
import type {
	AdminUserInvitationInfo,
	CreateUserInvitationRequest,
	CreateUserReq,
	UpdateUserRequest,
	UserRole,
	UserStatus,
} from "@/types/api";

interface GeneratedCreatePassword {
	password: string;
	username: string;
}

type CreateUserTextField = "email" | "password" | "username";

const USER_PAGE_SIZE_OPTIONS = [10, 20, 50] as const;
const DEFAULT_USER_PAGE_SIZE = 20 as const;
const USER_MANAGED_QUERY_KEYS = [
	"keyword",
	"offset",
	"pageSize",
	"role",
	"sortBy",
	"sortOrder",
	"status",
] as const;
const USER_SORT_BY_OPTIONS = [
	"id",
	"username",
	"email",
	"role",
	"status",
	"storage_used",
	"storage_quota",
	"created_at",
	"updated_at",
] as const satisfies readonly AdminUserSortBy[];
const DEFAULT_USER_SORT_BY = "created_at" as const satisfies AdminUserSortBy;
const DEFAULT_USER_SORT_ORDER = "desc" as const satisfies SortOrder;
function normalizeOffset(offset: number) {
	return Math.max(0, Math.floor(offset));
}

function parseRoleSearchParam(value: string | null): "__all__" | UserRole {
	return value === "admin" || value === "user" ? value : "__all__";
}

function parseStatusSearchParam(value: string | null): "__all__" | UserStatus {
	return value === "active" || value === "disabled" ? value : "__all__";
}

function buildManagedUserSearchParams({
	offset,
	pageSize,
	keyword,
	role,
	status,
	sortBy,
	sortOrder,
}: {
	offset: number;
	pageSize: (typeof USER_PAGE_SIZE_OPTIONS)[number];
	keyword: string;
	role: "__all__" | UserRole;
	status: "__all__" | UserStatus;
	sortBy: AdminUserSortBy;
	sortOrder: SortOrder;
}) {
	return buildOffsetPaginationSearchParams({
		offset,
		pageSize,
		defaultPageSize: DEFAULT_USER_PAGE_SIZE,
		extraParams: {
			keyword: keyword.trim() || undefined,
			role: role !== "__all__" ? role : undefined,
			sortBy: sortBy !== DEFAULT_USER_SORT_BY ? sortBy : undefined,
			sortOrder: sortOrder !== DEFAULT_USER_SORT_ORDER ? sortOrder : undefined,
			status: status !== "__all__" ? status : undefined,
		},
	});
}

function getManagedUserSearchString(searchParams: URLSearchParams) {
	return buildManagedUserSearchParams({
		offset: normalizeOffset(parseOffsetSearchParam(searchParams.get("offset"))),
		pageSize: parsePageSizeSearchParam(
			searchParams.get("pageSize"),
			USER_PAGE_SIZE_OPTIONS,
			DEFAULT_USER_PAGE_SIZE,
		),
		keyword: searchParams.get("keyword") ?? "",
		role: parseRoleSearchParam(searchParams.get("role")),
		status: parseStatusSearchParam(searchParams.get("status")),
		sortBy: parseSortSearchParam(
			searchParams.get("sortBy"),
			USER_SORT_BY_OPTIONS,
			DEFAULT_USER_SORT_BY,
		),
		sortOrder: parseSortOrderSearchParam(
			searchParams.get("sortOrder"),
			DEFAULT_USER_SORT_ORDER,
		),
	}).toString();
}

function mergeManagedUserSearchParams(
	searchParams: URLSearchParams,
	managedSearchParams: URLSearchParams,
) {
	const merged = new URLSearchParams(searchParams);
	for (const key of USER_MANAGED_QUERY_KEYS) {
		merged.delete(key);
	}
	for (const [key, value] of managedSearchParams.entries()) {
		merged.set(key, value);
	}
	return merged;
}

export default function AdminUsersPage() {
	const { t } = useTranslation("admin");
	usePageTitle(t("users"));
	const navigate = useNavigate();
	const [searchParams, setSearchParams] = useSearchParams();
	const initialKeyword = searchParams.get("keyword") ?? "";
	const initialRole = searchParams.get("role");
	const initialStatus = searchParams.get("status");
	const [offset, setOffsetState] = useState(
		normalizeOffset(parseOffsetSearchParam(searchParams.get("offset"))),
	);
	const [pageSize, setPageSize] = useState<
		(typeof USER_PAGE_SIZE_OPTIONS)[number]
	>(
		parsePageSizeSearchParam(
			searchParams.get("pageSize"),
			USER_PAGE_SIZE_OPTIONS,
			DEFAULT_USER_PAGE_SIZE,
		),
	);
	const [keyword, setKeyword] = useState(initialKeyword);
	const [debouncedKeyword, setDebouncedKeyword] = useState(initialKeyword);
	const [roleFilter, setRoleFilter] = useState<"__all__" | UserRole>(
		initialRole === "admin" || initialRole === "user" ? initialRole : "__all__",
	);
	const [statusFilter, setStatusFilter] = useState<"__all__" | UserStatus>(
		initialStatus === "active" || initialStatus === "disabled"
			? initialStatus
			: "__all__",
	);
	const [sortBy, setSortBy] = useState<AdminUserSortBy>(
		parseSortSearchParam(
			searchParams.get("sortBy"),
			USER_SORT_BY_OPTIONS,
			DEFAULT_USER_SORT_BY,
		),
	);
	const [sortOrder, setSortOrder] = useState<SortOrder>(
		parseSortOrderSearchParam(
			searchParams.get("sortOrder"),
			DEFAULT_USER_SORT_ORDER,
		),
	);
	const [detailDialogUserId, setDetailDialogUserId] = useState<number | null>(
		null,
	);
	const [createDialogOpen, setCreateDialogOpen] = useState(false);
	const [createErrors, setCreateErrors] = useState<Partial<CreateUserReq>>({});
	const [createForm, setCreateForm] = useState<CreateUserReq>({
		username: "",
		email: "",
		password: "",
		must_change_password: false,
	});
	const [generatedCreatePassword, setGeneratedCreatePassword] =
		useState<GeneratedCreatePassword | null>(null);
	const [inviteDialogOpen, setInviteDialogOpen] = useState(false);
	const [inviting, setInviting] = useState(false);
	const [inviteErrors, setInviteErrors] = useState<
		Partial<CreateUserInvitationRequest>
	>({});
	const [inviteForm, setInviteForm] = useState<CreateUserInvitationRequest>({
		email: "",
	});
	const [createdInvitation, setCreatedInvitation] =
		useState<AdminUserInvitationInfo | null>(null);
	const lastWrittenSearchRef = useRef<string | null>(null);
	const setOffset = (value: SetStateAction<number>) => {
		setOffsetState((current) =>
			normalizeOffset(typeof value === "function" ? value(current) : value),
		);
	};

	useEffect(() => {
		const timer = window.setTimeout(() => {
			setDebouncedKeyword(keyword);
		}, 300);
		return () => window.clearTimeout(timer);
	}, [keyword]);

	useEffect(() => {
		return runWhenIdle(() => {
			void loadAdminPolicyGroupLookup().catch(() => undefined);
		});
	}, []);

	useEffect(() => {
		const managedSearch = getManagedUserSearchString(searchParams);
		if (managedSearch === lastWrittenSearchRef.current) {
			return;
		}

		const nextOffset = normalizeOffset(
			parseOffsetSearchParam(searchParams.get("offset")),
		);
		const nextPageSize = parsePageSizeSearchParam(
			searchParams.get("pageSize"),
			USER_PAGE_SIZE_OPTIONS,
			DEFAULT_USER_PAGE_SIZE,
		);
		const nextKeyword = searchParams.get("keyword") ?? "";
		const nextRole = parseRoleSearchParam(searchParams.get("role"));
		const nextStatus = parseStatusSearchParam(searchParams.get("status"));
		const nextSortBy = parseSortSearchParam(
			searchParams.get("sortBy"),
			USER_SORT_BY_OPTIONS,
			DEFAULT_USER_SORT_BY,
		);
		const nextSortOrder = parseSortOrderSearchParam(
			searchParams.get("sortOrder"),
			DEFAULT_USER_SORT_ORDER,
		);

		setOffsetState((prev) => (prev === nextOffset ? prev : nextOffset));
		setPageSize((prev) => (prev === nextPageSize ? prev : nextPageSize));
		setKeyword((prev) => (prev === nextKeyword ? prev : nextKeyword));
		setDebouncedKeyword((prev) => (prev === nextKeyword ? prev : nextKeyword));
		setRoleFilter((prev) => (prev === nextRole ? prev : nextRole));
		setStatusFilter((prev) => (prev === nextStatus ? prev : nextStatus));
		setSortBy((prev) => (prev === nextSortBy ? prev : nextSortBy));
		setSortOrder((prev) => (prev === nextSortOrder ? prev : nextSortOrder));
	}, [searchParams]);

	useEffect(() => {
		const nextManagedSearchParams = buildManagedUserSearchParams({
			offset,
			pageSize,
			keyword: debouncedKeyword,
			role: roleFilter,
			status: statusFilter,
			sortBy,
			sortOrder,
		});
		const nextSearch = nextManagedSearchParams.toString();
		const currentSearch = getManagedUserSearchString(searchParams);
		if (
			currentSearch !== lastWrittenSearchRef.current &&
			currentSearch !== nextSearch
		) {
			return;
		}

		lastWrittenSearchRef.current = nextSearch;
		if (nextSearch === currentSearch) {
			return;
		}

		setSearchParams(
			mergeManagedUserSearchParams(searchParams, nextManagedSearchParams),
			{ replace: true },
		);
	}, [
		debouncedKeyword,
		offset,
		pageSize,
		roleFilter,
		searchParams,
		setSearchParams,
		sortBy,
		sortOrder,
		statusFilter,
	]);

	const {
		items: users,
		loading,
		reload: reloadUsers,
		setItems: setUsers,
		setTotal,
		total,
	} = useApiList(
		() =>
			adminUserService.list({
				limit: pageSize,
				offset,
				keyword: debouncedKeyword.trim() || undefined,
				role: roleFilter === "__all__" ? undefined : roleFilter,
				status: statusFilter === "__all__" ? undefined : statusFilter,
				sort_by: sortBy,
				sort_order: sortOrder,
			}),
		[
			debouncedKeyword,
			offset,
			pageSize,
			roleFilter,
			sortBy,
			sortOrder,
			statusFilter,
		],
	);
	const { pendingId: deletingUserId, runWithPending: runWithDeletingUser } =
		usePendingId<number>();
	const { pending: creating, runWithPending: runWithCreatingUser } =
		usePendingAction();

	const activeFilterCount =
		(debouncedKeyword.trim().length > 0 ? 1 : 0) +
		(roleFilter !== "__all__" ? 1 : 0) +
		(statusFilter !== "__all__" ? 1 : 0);
	const totalPages = Math.max(1, Math.ceil(total / pageSize));
	const currentPage = Math.floor(offset / pageSize) + 1;
	const hasServerFilters = activeFilterCount > 0;
	const prevPageDisabled = offset === 0;
	const nextPageDisabled = offset + pageSize >= total;

	const resetFilters = () => {
		setKeyword("");
		setDebouncedKeyword("");
		setRoleFilter("__all__");
		setStatusFilter("__all__");
		setOffset(0);
	};

	const handlePageSizeChange = (value: string | null) => {
		const next = parsePageSizeOption(value, USER_PAGE_SIZE_OPTIONS);
		if (next == null) return;
		setPageSize(next);
		setOffset(0);
	};

	const handleKeywordChange = (value: string) => {
		setKeyword(value);
		setOffset(0);
	};

	const handleRoleFilterChange = (value: string | null) => {
		if (!value) return;
		setRoleFilter(value as "__all__" | UserRole);
		setOffset(0);
	};

	const handleStatusFilterChange = (value: string | null) => {
		if (!value) return;
		setStatusFilter(value as "__all__" | UserStatus);
		setOffset(0);
	};

	const handleSortChange = (
		nextSortBy: AdminUserSortBy,
		nextOrder: SortOrder,
	) => {
		setSortBy(nextSortBy);
		setSortOrder(nextOrder);
		setOffset(0);
	};

	const resetCreateForm = () => {
		setCreateForm({
			username: "",
			email: "",
			password: "",
			must_change_password: false,
		});
		setCreateErrors({});
	};

	const validateCreateField = (field: CreateUserTextField, value: string) => {
		const schema =
			field === "username"
				? usernameSchema
				: field === "email"
					? emailSchema
					: passwordSchema;
		const result = schema.safeParse(value);
		setCreateErrors((prev) => {
			if (result.success) {
				const next = { ...prev };
				delete next[field];
				return next;
			}
			return { ...prev, [field]: result.error.issues[0]?.message ?? "" };
		});
	};

	const validateCreateForm = () => {
		const nextErrors: Partial<CreateUserReq> = {};
		const usernameResult = usernameSchema.safeParse(createForm.username.trim());
		if (!usernameResult.success) {
			nextErrors.username = usernameResult.error.issues[0]?.message ?? "";
		}
		const emailResult = emailSchema.safeParse(createForm.email.trim());
		if (!emailResult.success) {
			nextErrors.email = emailResult.error.issues[0]?.message ?? "";
		}
		const trimmedCreatePassword = (createForm.password ?? "").trim();
		if (trimmedCreatePassword.length > 0) {
			const passwordResult = passwordSchema.safeParse(trimmedCreatePassword);
			if (!passwordResult.success) {
				nextErrors.password = passwordResult.error.issues[0]?.message ?? "";
			}
		}
		setCreateErrors(nextErrors);
		return Object.keys(nextErrors).length === 0;
	};

	const handleCreateFormChange = (
		key: keyof CreateUserReq,
		value: boolean | string,
	) => {
		setCreateForm((prev) => ({ ...prev, [key]: value }));
	};

	const copyGeneratedPassword = async () => {
		if (!generatedCreatePassword?.password) {
			return;
		}
		try {
			await writeTextToClipboard(generatedCreatePassword.password);
			toast.success(t("core:copied_to_clipboard"));
		} catch (error) {
			handleApiError(error);
		}
	};

	const resetInviteForm = () => {
		setInviteForm({ email: "" });
		setInviteErrors({});
		setCreatedInvitation(null);
	};

	const validateInviteField = (
		field: keyof CreateUserInvitationRequest,
		value: string,
	) => {
		const result = emailSchema.safeParse(value);
		setInviteErrors((prev) => {
			if (result.success) {
				const next = { ...prev };
				delete next[field];
				return next;
			}
			return { ...prev, [field]: result.error.issues[0]?.message ?? "" };
		});
	};

	const validateInviteForm = () => {
		const nextErrors: Partial<CreateUserInvitationRequest> = {};
		const emailResult = emailSchema.safeParse(inviteForm.email.trim());
		if (!emailResult.success) {
			nextErrors.email = emailResult.error.issues[0]?.message ?? "";
		}
		setInviteErrors(nextErrors);
		return Object.keys(nextErrors).length === 0;
	};

	const handleInviteFormChange = (
		key: keyof CreateUserInvitationRequest,
		value: string,
	) => {
		setInviteForm((prev) => ({ ...prev, [key]: value }));
		if (createdInvitation) {
			setCreatedInvitation(null);
		}
	};

	const copyInvitationLink = async (value: string) => {
		if (!value) {
			return;
		}
		try {
			await writeTextToClipboard(value);
			toast.success(t("core:copied_to_clipboard"));
		} catch (error) {
			handleApiError(error);
		}
	};

	const handleCreateUser = async (event: FormEvent<HTMLFormElement>) => {
		event.preventDefault();
		if (!validateCreateForm()) return;
		try {
			const result = await runWithCreatingUser(async () => {
				const trimmedCreatePassword = (createForm.password ?? "").trim();
				const password = trimmedCreatePassword || undefined;
				const result = await adminUserService.create({
					username: createForm.username.trim(),
					email: createForm.email.trim(),
					password,
					must_change_password: createForm.must_change_password,
				});
				toast.success(t("user_created"));
				setCreateDialogOpen(false);
				resetCreateForm();
				if (result.generated_password) {
					setGeneratedCreatePassword({
						password: result.generated_password,
						username: result.user.username,
					});
				}
				await reloadUsers();
			});
			if (!result.entered) return;
		} catch (e) {
			handleApiError(e);
		}
	};

	const handleInviteUser = async (event: FormEvent<HTMLFormElement>) => {
		event.preventDefault();
		if (!validateInviteForm()) return;
		try {
			setInviting(true);
			const invitation = await adminUserService.createInvitation({
				email: inviteForm.email.trim(),
			});
			setCreatedInvitation(invitation);
			setInviteForm({ email: invitation.email });
			toast.success(t("invitation_created"));
		} catch (e) {
			handleApiError(e);
		} finally {
			setInviting(false);
		}
	};

	const updateUser = async (id: number, data: UpdateUserRequest) => {
		try {
			await adminUserService.update(id, data);
			await reloadUsers();
			toast.success(t("user_updated"));
		} catch (e) {
			handleApiError(e);
		}
	};

	const deleteUser = async (id: number) => {
		await runWithDeletingUser(id, async () => {
			try {
				await adminUserService.delete(id);
				const isLastItemOnPage = users.length === 1;
				const nextOffset =
					isLastItemOnPage && offset > 0
						? Math.max(0, offset - pageSize)
						: offset;
				if (detailDialogUserId === id) {
					setDetailDialogUserId(null);
				}
				if (nextOffset !== offset) {
					setOffset(nextOffset);
				} else {
					setUsers((prev) => prev.filter((u) => u.id !== id));
					setTotal((prev) => Math.max(0, prev - 1));
				}
				toast.success(t("user_deleted"));
			} catch (e) {
				handleApiError(e);
			}
		});
	};
	const {
		confirmId: deleteUserId,
		requestConfirm: requestDeleteUserConfirm,
		dialogProps: deleteDialogProps,
	} = useConfirmDialog<number>(async (id) => {
		if (id !== 1) {
			await deleteUser(id);
		}
	});

	const selectedUser = useMemo(
		() => users.find((user) => user.id === detailDialogUserId) ?? null,
		[users, detailDialogUserId],
	);
	const deleteTargetUser = useMemo(
		() => users.find((user) => user.id === deleteUserId) ?? null,
		[users, deleteUserId],
	);
	const roleFilterOptions = [
		{ label: t("all_roles"), value: "__all__" },
		{ label: t("role_admin"), value: "admin" },
		{ label: t("role_user"), value: "user" },
	] satisfies ReadonlyArray<{ label: string; value: string }>;
	const statusFilterOptions = [
		{ label: t("all_statuses"), value: "__all__" },
		{ label: t("core:active"), value: "active" },
		{ label: t("core:disabled_status"), value: "disabled" },
	] satisfies ReadonlyArray<{ label: string; value: string }>;
	const pageSizeOptions = USER_PAGE_SIZE_OPTIONS.map((size) => ({
		label: t("page_size_option", { count: size }),
		value: String(size),
	}));
	const usersEmptyIcon = <Icon name="ListBullets" className="size-10" />;
	const usersFilteredEmptyAction = (
		<Button variant="outline" onClick={resetFilters}>
			{t("clear_filters")}
		</Button>
	);
	const usersTableHeader = (
		<UsersTableHeader
			sortBy={sortBy}
			sortOrder={sortOrder}
			onSortChange={handleSortChange}
		/>
	);
	const usersPagination = (
		<AdminOffsetPagination
			total={total}
			currentPage={currentPage}
			totalPages={totalPages}
			pageSize={String(pageSize)}
			pageSizeOptions={pageSizeOptions}
			onPageSizeChange={handlePageSizeChange}
			prevDisabled={prevPageDisabled}
			nextDisabled={nextPageDisabled}
			onPrevious={() => setOffset((current) => Math.max(0, current - pageSize))}
			onNext={() => setOffset((current) => current + pageSize)}
		/>
	);

	return (
		<AdminLayout>
			<AdminPageShell>
				<AdminPageHeader
					title={t("users")}
					description={t("users_intro")}
					actions={
						<>
							<Button
								variant="outline"
								size="sm"
								className={ADMIN_CONTROL_HEIGHT_CLASS}
								onClick={() => setInviteDialogOpen(true)}
							>
								<Icon name="EnvelopeSimple" className="mr-1 size-4" />
								{t("invite_user")}
							</Button>
							<Button
								variant="outline"
								size="sm"
								className={ADMIN_CONTROL_HEIGHT_CLASS}
								onClick={() => navigate("/admin/users/invitations")}
							>
								<Icon name="ListBullets" className="mr-1 size-4" />
								{t("invitation_records")}
							</Button>
							<Button
								size="sm"
								className={ADMIN_CONTROL_HEIGHT_CLASS}
								onClick={() => setCreateDialogOpen(true)}
							>
								<Icon name="Plus" className="mr-1 size-4" />
								{t("new_user")}
							</Button>
							<Button
								variant="outline"
								size="sm"
								className={ADMIN_CONTROL_HEIGHT_CLASS}
								onClick={() => void reloadUsers()}
								disabled={loading}
							>
								<Icon
									name={loading ? "Spinner" : "ArrowsClockwise"}
									className={`mr-1 size-3.5 ${loading ? "animate-spin" : ""}`}
								/>
								{t("core:refresh")}
							</Button>
						</>
					}
					toolbar={
						<UsersToolbar
							activeFilterCount={activeFilterCount}
							keyword={keyword}
							roleFilter={roleFilter}
							roleFilterOptions={roleFilterOptions}
							statusFilter={statusFilter}
							statusFilterOptions={statusFilterOptions}
							onKeywordChange={handleKeywordChange}
							onResetFilters={resetFilters}
							onRoleFilterChange={handleRoleFilterChange}
							onStatusFilterChange={handleStatusFilterChange}
						/>
					}
				/>
				<AdminTableList
					loading={loading}
					items={users}
					columns={7}
					rows={6}
					emptyIcon={usersEmptyIcon}
					emptyTitle={t("no_users")}
					filtered={hasServerFilters}
					filteredEmptyTitle={t("no_filtered_users")}
					filteredEmptyDescription={t("no_filtered_users_desc")}
					filteredEmptyAction={usersFilteredEmptyAction}
					headerRow={usersTableHeader}
					pagination={usersPagination}
					renderRow={(user) => (
						<UsersTableRow
							key={user.id}
							deletingUserId={deletingUserId}
							onDeleteUser={requestDeleteUserConfirm}
							onOpenUserDetail={setDetailDialogUserId}
							user={user}
						/>
					)}
				/>
			</AdminPageShell>
			<CreateUserDialog
				open={createDialogOpen}
				onOpenChange={(open) => {
					setCreateDialogOpen(open);
					if (!open && !creating) {
						resetCreateForm();
					}
				}}
				form={createForm}
				createErrors={createErrors}
				creating={creating}
				onFieldChange={handleCreateFormChange}
				onFieldValidate={validateCreateField}
				onSubmit={handleCreateUser}
			/>
			<GeneratedPasswordDialog
				open={generatedCreatePassword !== null}
				password={generatedCreatePassword?.password ?? null}
				username={generatedCreatePassword?.username ?? ""}
				onCopy={() => void copyGeneratedPassword()}
				onOpenChange={(open) => {
					if (!open) {
						setGeneratedCreatePassword(null);
					}
				}}
			/>
			<InviteUserDialog
				open={inviteDialogOpen}
				onOpenChange={(open) => {
					setInviteDialogOpen(open);
					if (!open && !inviting) {
						resetInviteForm();
					}
				}}
				form={inviteForm}
				errors={inviteErrors}
				inviting={inviting}
				createdInvitation={createdInvitation}
				onCopyLink={(value) => void copyInvitationLink(value)}
				onFieldChange={handleInviteFormChange}
				onFieldValidate={validateInviteField}
				onSubmit={handleInviteUser}
			/>
			<UserDetailDialog
				user={selectedUser}
				open={detailDialogUserId !== null}
				onOpenChange={(open) => {
					if (!open) setDetailDialogUserId(null);
				}}
				onUpdate={updateUser}
			/>
			<ConfirmDialog
				{...deleteDialogProps}
				title={t("delete_user")}
				description={
					deleteTargetUser?.id === 1
						? t("initial_admin_delete_blocked")
						: t("confirm_force_delete")
				}
				confirmLabel={t("core:delete")}
				variant="destructive"
			/>
		</AdminLayout>
	);
}
