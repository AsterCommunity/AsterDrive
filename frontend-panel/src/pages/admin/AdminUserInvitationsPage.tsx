import type { FormEvent, SetStateAction } from "react";
import { useCallback, useMemo, useReducer } from "react";
import { useTranslation } from "react-i18next";
import { useNavigate, useSearchParams } from "react-router-dom";
import { toast } from "sonner";
import { AdminOffsetPagination } from "@/components/admin/AdminOffsetPagination";
import { InviteUserDialog } from "@/components/admin/admin-users-page/InviteUserDialog";
import {
	UserInvitationsTableHeader,
	UserInvitationsTableRow,
} from "@/components/admin/admin-users-page/UserInvitationsTable";
import { AdminTableList } from "@/components/common/AdminTableList";
import { ConfirmDialog } from "@/components/common/ConfirmDialog";
import { AdminLayout } from "@/components/layout/AdminLayout";
import { AdminPageHeader } from "@/components/layout/AdminPageHeader";
import { AdminPageShell } from "@/components/layout/AdminPageShell";
import { Button } from "@/components/ui/button";
import { Icon } from "@/components/ui/icon";
import { handleApiError } from "@/hooks/useApiError";
import { useConfirmDialog } from "@/hooks/useConfirmDialog";
import { useManagedAdminList } from "@/hooks/useManagedAdminList";
import {
	type ManagedListQuerySchema,
	managedOffsetQueryField,
	managedPageSizeQueryField,
	useManagedListQueryState,
} from "@/hooks/useManagedListQueryState";
import { usePageTitle } from "@/hooks/usePageTitle";
import { usePendingId } from "@/hooks/usePendingId";
import { writeTextToClipboard } from "@/lib/clipboard";
import { ADMIN_CONTROL_HEIGHT_CLASS } from "@/lib/constants";
import { parsePageSizeOption } from "@/lib/pagination";
import { emailSchema } from "@/lib/validation";
import { adminUserService } from "@/services/adminService";
import type {
	AdminUserInvitationInfo,
	CreateUserInvitationRequest,
} from "@/types/api";

const INVITATION_PAGE_SIZE_OPTIONS = [10, 20, 50] as const;
const DEFAULT_INVITATION_PAGE_SIZE = 20 as const;
type InvitationPageSize = (typeof INVITATION_PAGE_SIZE_OPTIONS)[number];
type ManagedInvitationQuery = {
	offset: number;
	pageSize: InvitationPageSize;
};

const MANAGED_INVITATION_QUERY_DEFAULTS = {
	offset: 0,
	pageSize: DEFAULT_INVITATION_PAGE_SIZE,
} satisfies ManagedInvitationQuery;

const MANAGED_INVITATION_QUERY_SCHEMA = {
	offset: managedOffsetQueryField(),
	pageSize: managedPageSizeQueryField(
		INVITATION_PAGE_SIZE_OPTIONS,
		DEFAULT_INVITATION_PAGE_SIZE,
	),
} satisfies ManagedListQuerySchema<ManagedInvitationQuery>;

type AdminUserInvitationsPageState = {
	createdInvitation: AdminUserInvitationInfo | null;
	inviteDialogOpen: boolean;
	inviteErrors: Partial<CreateUserInvitationRequest>;
	inviteForm: CreateUserInvitationRequest;
	inviting: boolean;
};

type AdminUserInvitationsPageAction =
	| { type: "closeInviteDialog" }
	| { type: "inviteCreated"; invitation: AdminUserInvitationInfo }
	| {
			type: "inviteFieldChanged";
			key: keyof CreateUserInvitationRequest;
			value: string;
	  }
	| {
			type: "inviteFieldErrorCleared";
			field: keyof CreateUserInvitationRequest;
	  }
	| {
			type: "inviteFieldErrorSet";
			field: keyof CreateUserInvitationRequest;
			message: string;
	  }
	| {
			type: "inviteFormErrorsSet";
			errors: Partial<CreateUserInvitationRequest>;
	  }
	| { type: "inviteFormReset" }
	| { type: "inviteOpenSet"; open: boolean }
	| { type: "invitingSet"; inviting: boolean };

function createInitialInviteForm(): CreateUserInvitationRequest {
	return { email: "" };
}

function initAdminUserInvitationsPageState(): AdminUserInvitationsPageState {
	return {
		createdInvitation: null,
		inviteDialogOpen: false,
		inviteErrors: {},
		inviteForm: createInitialInviteForm(),
		inviting: false,
	};
}

function adminUserInvitationsPageReducer(
	state: AdminUserInvitationsPageState,
	action: AdminUserInvitationsPageAction,
): AdminUserInvitationsPageState {
	switch (action.type) {
		case "closeInviteDialog":
			return {
				...state,
				inviteDialogOpen: false,
				...(state.inviting
					? {}
					: {
							createdInvitation: null,
							inviteErrors: {},
							inviteForm: createInitialInviteForm(),
						}),
			};
		case "inviteCreated":
			return {
				...state,
				createdInvitation: action.invitation,
				inviteForm: { email: action.invitation.email },
			};
		case "inviteFieldChanged":
			return {
				...state,
				createdInvitation: null,
				inviteForm: { ...state.inviteForm, [action.key]: action.value },
			};
		case "inviteFieldErrorCleared": {
			const nextErrors = { ...state.inviteErrors };
			delete nextErrors[action.field];
			return { ...state, inviteErrors: nextErrors };
		}
		case "inviteFieldErrorSet":
			return {
				...state,
				inviteErrors: {
					...state.inviteErrors,
					[action.field]: action.message,
				},
			};
		case "inviteFormErrorsSet":
			return { ...state, inviteErrors: action.errors };
		case "inviteFormReset":
			return {
				...state,
				createdInvitation: null,
				inviteErrors: {},
				inviteForm: createInitialInviteForm(),
			};
		case "inviteOpenSet":
			return { ...state, inviteDialogOpen: action.open };
		case "invitingSet":
			return { ...state, inviting: action.inviting };
	}
}

export default function AdminUserInvitationsPage() {
	const { t } = useTranslation("admin");
	usePageTitle(t("user_invitations"));
	const navigate = useNavigate();
	const [searchParams, setSearchParams] = useSearchParams();
	const [state, dispatch] = useReducer(
		adminUserInvitationsPageReducer,
		undefined,
		initAdminUserInvitationsPageState,
	);
	const {
		createdInvitation,
		inviteDialogOpen,
		inviteErrors,
		inviteForm,
		inviting,
	} = state;
	const { query, setQuery } = useManagedListQueryState({
		defaults: MANAGED_INVITATION_QUERY_DEFAULTS,
		schema: MANAGED_INVITATION_QUERY_SCHEMA,
		searchParams,
		setSearchParams,
	});
	const { offset, pageSize } = query;
	const setOffset = useCallback(
		(value: SetStateAction<number>) => {
			setQuery((current) => ({
				offset: Math.max(
					0,
					typeof value === "function" ? value(current.offset) : value,
				),
			}));
		},
		[setQuery],
	);
	const {
		currentPage,
		items: invitations,
		loading,
		reload,
		setItems: setInvitations,
		total,
		totalPages,
		nextPageDisabled,
		prevPageDisabled,
	} = useManagedAdminList<AdminUserInvitationInfo, ManagedInvitationQuery>({
		deps: [offset, pageSize],
		loadPage: (query) =>
			adminUserService.listInvitations({
				limit: query.pageSize,
				offset: query.offset,
			}),
		query,
		setOffset,
	});
	const {
		pendingId: revokingInvitationId,
		runWithPending: runWithRevokingInvitation,
	} = usePendingId<number>();

	const outOfRangeEmptyPage = invitations.length === 0 && total > 0;
	const pageSizeOptions = INVITATION_PAGE_SIZE_OPTIONS.map((size) => ({
		label: t("page_size_option", { count: size }),
		value: String(size),
	}));

	const validateInviteField = (
		field: keyof CreateUserInvitationRequest,
		value: string,
	) => {
		const result = emailSchema.safeParse(value);
		dispatch(
			result.success
				? { type: "inviteFieldErrorCleared", field }
				: {
						type: "inviteFieldErrorSet",
						field,
						message: result.error.issues[0]?.message ?? "",
					},
		);
	};

	const validateInviteForm = () => {
		const nextErrors: Partial<CreateUserInvitationRequest> = {};
		const emailResult = emailSchema.safeParse(inviteForm.email.trim());
		if (!emailResult.success) {
			nextErrors.email = emailResult.error.issues[0]?.message ?? "";
		}
		dispatch({ type: "inviteFormErrorsSet", errors: nextErrors });
		return Object.keys(nextErrors).length === 0;
	};

	const handleInviteFormChange = (
		key: keyof CreateUserInvitationRequest,
		value: string,
	) => {
		dispatch({ type: "inviteFieldChanged", key, value });
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

	const handleInviteUser = async (event: FormEvent<HTMLFormElement>) => {
		event.preventDefault();
		if (!validateInviteForm()) return;
		try {
			dispatch({ type: "invitingSet", inviting: true });
			const invitation = await adminUserService.createInvitation({
				email: inviteForm.email.trim(),
			});
			dispatch({ type: "inviteCreated", invitation });
			toast.success(t("invitation_created"));
			if (offset !== 0) {
				setOffset(0);
			} else {
				await reload();
			}
		} catch (error) {
			handleApiError(error);
		} finally {
			dispatch({ type: "invitingSet", inviting: false });
		}
	};

	const {
		confirmId: revokeInvitationId,
		requestConfirm: requestRevokeInvitationConfirm,
		dialogProps: revokeInvitationDialogProps,
	} = useConfirmDialog<number>(async (id) => {
		await runWithRevokingInvitation(id, async () => {
			try {
				const invitation = await adminUserService.revokeInvitation(id);
				setInvitations((prev) =>
					prev.map((item) => (item.id === id ? invitation : item)),
				);
				toast.success(t("invitation_revoked"));
			} catch (error) {
				handleApiError(error);
			}
		});
	});

	const revokeTargetInvitation = useMemo(
		() =>
			invitations.find((invitation) => invitation.id === revokeInvitationId) ??
			null,
		[invitations, revokeInvitationId],
	);

	const handlePageSizeChange = (value: string | null) => {
		const next = parsePageSizeOption(value, INVITATION_PAGE_SIZE_OPTIONS);
		if (next == null) return;
		setQuery({ offset: 0, pageSize: next });
	};
	const invitationsPagination = (
		<AdminOffsetPagination
			total={total}
			currentPage={currentPage}
			totalPages={totalPages}
			pageSize={String(pageSize)}
			pageSizeOptions={pageSizeOptions}
			onPageSizeChange={handlePageSizeChange}
			prevDisabled={prevPageDisabled}
			nextDisabled={nextPageDisabled}
			onPrevious={() => setOffset((current) => current - pageSize)}
			onNext={() => setOffset((current) => current + pageSize)}
		/>
	);
	const invitationsEmptyIcon = (
		<Icon name="EnvelopeSimple" className="size-10" />
	);
	const invitationsEmptyAction = (
		<Button onClick={() => dispatch({ type: "inviteOpenSet", open: true })}>
			<Icon name="EnvelopeSimple" className="mr-1 size-4" />
			{t("invite_user")}
		</Button>
	);
	const invitationsTableHeader = <UserInvitationsTableHeader />;

	return (
		<AdminLayout>
			<AdminPageShell>
				<AdminPageHeader
					title={t("user_invitations")}
					description={t("user_invitations_intro")}
					actions={
						<>
							<Button
								variant="outline"
								size="sm"
								className={ADMIN_CONTROL_HEIGHT_CLASS}
								onClick={() => navigate("/admin/users")}
							>
								<Icon name="CaretLeft" className="mr-1 size-4" />
								{t("back_to_users")}
							</Button>
							<Button
								size="sm"
								className={ADMIN_CONTROL_HEIGHT_CLASS}
								onClick={() => dispatch({ type: "inviteOpenSet", open: true })}
							>
								<Icon name="EnvelopeSimple" className="mr-1 size-4" />
								{t("invite_user")}
							</Button>
							<Button
								variant="outline"
								size="sm"
								className={ADMIN_CONTROL_HEIGHT_CLASS}
								onClick={() => void reload()}
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
				/>

				<AdminTableList
					loading={loading || outOfRangeEmptyPage}
					items={invitations}
					columns={6}
					rows={6}
					emptyIcon={invitationsEmptyIcon}
					emptyTitle={t("no_invitations")}
					emptyDescription={t("no_invitations_desc")}
					emptyAction={invitationsEmptyAction}
					headerRow={invitationsTableHeader}
					pagination={invitationsPagination}
					renderRow={(invitation) => (
						<UserInvitationsTableRow
							key={invitation.id}
							invitation={invitation}
							revokingInvitationId={revokingInvitationId}
							onRevokeInvitation={(item) =>
								requestRevokeInvitationConfirm(item.id)
							}
						/>
					)}
				/>
			</AdminPageShell>
			<InviteUserDialog
				open={inviteDialogOpen}
				onOpenChange={(open) => {
					if (open) {
						dispatch({ type: "inviteOpenSet", open });
						return;
					}
					dispatch({ type: "closeInviteDialog" });
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
			<ConfirmDialog
				{...revokeInvitationDialogProps}
				title={t("revoke_invitation")}
				description={t("confirm_revoke_invitation", {
					email: revokeTargetInvitation?.email ?? "",
				})}
				confirmLabel={t("revoke_invitation")}
				variant="destructive"
			/>
		</AdminLayout>
	);
}
