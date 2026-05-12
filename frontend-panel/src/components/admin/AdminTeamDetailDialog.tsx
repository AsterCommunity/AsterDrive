import { type FormEvent, useEffect, useMemo, useRef, useState } from "react";
import { useTranslation } from "react-i18next";
import { toast } from "sonner";
import {
	AdminTeamDetailAuditSection,
	AdminTeamDetailDangerSection,
	AdminTeamDetailMembersSection,
	AdminTeamDetailOverviewSection,
} from "@/components/admin/admin-team-detail/AdminTeamDetailSections";
import { AdminTeamDetailShell } from "@/components/admin/admin-team-detail/AdminTeamDetailShell";
import {
	ADMIN_TEAM_DETAIL_AUDIT_PAGE_SIZE,
	ADMIN_TEAM_DETAIL_MEMBER_PAGE_SIZE,
	adminTeamDetailContentScrollPositions,
	adminTeamDetailSidebarScrollPositions,
	buildPolicyGroupOptions,
} from "@/components/admin/admin-team-detail/adminTeamDetailDialogState";
import type { AdminTeamDetailTab } from "@/components/admin/admin-team-detail/types";
import { useAdminTeamDetailData } from "@/components/admin/admin-team-detail/useAdminTeamDetailData";
import { useAdminTeamDetailScrollRestoration } from "@/components/admin/admin-team-detail/useAdminTeamDetailScrollRestoration";
import { useAdminTeamDetailTabs } from "@/components/admin/admin-team-detail/useAdminTeamDetailTabs";
import { ConfirmDialog } from "@/components/common/ConfirmDialog";
import { handleApiError } from "@/hooks/useApiError";
import { useConfirmDialog } from "@/hooks/useConfirmDialog";
import type { SortOrder } from "@/lib/pagination";
import { getUserDisplayName } from "@/lib/user";
import { adminTeamService } from "@/services/adminService";
import type { AdminTeamMemberSortBy } from "@/types/adminSort";
import type {
	StoragePolicyGroup,
	TeamMemberRole,
	UserStatus,
} from "@/types/api";

export type { AdminTeamDetailTab } from "@/components/admin/admin-team-detail/types";

interface AdminTeamDetailDialogProps {
	layout?: "dialog" | "page";
	open: boolean;
	teamId: number | null;
	policyGroups: StoragePolicyGroup[];
	policyGroupsLoading: boolean;
	onListChange: () => Promise<void>;
	onOpenChange: (open: boolean) => void;
	onPageTabChange?: (
		tab: AdminTeamDetailTab,
		options?: { replace?: boolean },
	) => void;
	onRefreshPolicyGroups: () => Promise<void>;
	pageTab?: AdminTeamDetailTab;
}

export function AdminTeamDetailDialog({
	layout = "dialog",
	open,
	teamId,
	policyGroups,
	policyGroupsLoading,
	onListChange,
	onOpenChange,
	onPageTabChange,
	onRefreshPolicyGroups,
	pageTab,
}: AdminTeamDetailDialogProps) {
	const { t } = useTranslation(["admin", "core", "settings"]);
	const isPageLayout = layout === "page";
	const { currentTab, handleTabChange, panelAnimationClass, resetDialogTab } =
		useAdminTeamDetailTabs({
			isPageLayout,
			onPageTabChange,
			pageTab,
		});
	const [archiveConfirmValue, setArchiveConfirmValue] = useState("");
	const [archiving, setArchiving] = useState(false);
	const [auditOffset, setAuditOffset] = useState(0);
	const [memberIdentifier, setMemberIdentifier] = useState("");
	const [memberMutating, setMemberMutating] = useState(false);
	const [memberOffset, setMemberOffset] = useState(0);
	const [memberQuery, setMemberQuery] = useState("");
	const [memberRole, setMemberRole] = useState<TeamMemberRole>("member");
	const [memberRoleFilter, setMemberRoleFilter] = useState<
		"__all__" | TeamMemberRole
	>("__all__");
	const [memberSortBy, setMemberSortBy] =
		useState<AdminTeamMemberSortBy>("role");
	const [memberSortOrder, setMemberSortOrder] = useState<SortOrder>("asc");
	const [memberStatusFilter, setMemberStatusFilter] = useState<
		"__all__" | UserStatus
	>("__all__");
	const [name, setName] = useState("");
	const [description, setDescription] = useState("");
	const [policyGroupId, setPolicyGroupId] = useState("");
	const [restoring, setRestoring] = useState(false);
	const [saving, setSaving] = useState(false);
	const contentRef = useRef<HTMLDivElement | null>(null);
	const overviewSyncAllowedRef = useRef(true);
	const sidebarRef = useRef<HTMLElement | null>(null);
	const roleOptions: TeamMemberRole[] = ["owner", "admin", "member"];
	const statusFilterOptions = [
		{ label: t("member_status_filter_all"), value: "__all__" },
		{ label: t("core:active"), value: "active" },
		{ label: t("core:disabled_status"), value: "disabled" },
	] satisfies ReadonlyArray<{
		label: string;
		value: "__all__" | UserStatus;
	}>;
	const roleFilterOptions = [
		{ label: t("member_role_filter_all"), value: "__all__" },
		...roleOptions.map((role) => ({
			label: t(`settings:settings_team_role_${role}`),
			value: role,
		})),
	] satisfies ReadonlyArray<{
		label: string;
		value: "__all__" | TeamMemberRole;
	}>;

	const roleLabel = (role: TeamMemberRole) =>
		t(`settings:settings_team_role_${role}`);
	const memberKeyword = memberQuery.trim();
	const memberRoleValue =
		memberRoleFilter === "__all__" ? undefined : memberRoleFilter;
	const memberStatusValue =
		memberStatusFilter === "__all__" ? undefined : memberStatusFilter;
	const memberFilters = useMemo(
		() => ({
			keyword: memberKeyword || undefined,
			role: memberRoleValue,
			status: memberStatusValue,
		}),
		[memberKeyword, memberRoleValue, memberStatusValue],
	);
	const {
		auditEntries,
		auditLoading,
		auditTotal,
		detailLoading,
		loadAuditEntries,
		loadMembers,
		loadTeamDetail,
		managerCount,
		memberLoading,
		memberTotal,
		members,
		ownerCount,
		team,
	} = useAdminTeamDetailData({
		auditOffset,
		memberFilters,
		memberOffset,
		memberSortBy,
		memberSortOrder,
		open,
		teamId,
	});

	useAdminTeamDetailScrollRestoration({
		contentRef,
		isPageLayout,
		pageTab,
		sidebarRef,
		teamId,
	});

	useEffect(() => {
		setArchiveConfirmValue("");
		if (!overviewSyncAllowedRef.current) {
			return;
		}

		setName(team?.name ?? "");
		setDescription(team?.description ?? "");
		setPolicyGroupId(
			team?.policy_group_id != null ? String(team.policy_group_id) : "",
		);
	}, [team]);

	const handleNameChange = (value: string) => {
		overviewSyncAllowedRef.current = false;
		setName(value);
	};

	const handleDescriptionChange = (value: string) => {
		overviewSyncAllowedRef.current = false;
		setDescription(value);
	};

	const handlePolicyGroupChange = (value: string) => {
		overviewSyncAllowedRef.current = false;
		setPolicyGroupId(value);
	};

	const quota = team?.storage_quota ?? 0;
	const used = team?.storage_used ?? 0;
	const usagePercentage = quota > 0 ? Math.min((used / quota) * 100, 100) : 0;
	const selectedPolicyGroupId = policyGroupId ? Number(policyGroupId) : null;
	const policyGroupOptions = buildPolicyGroupOptions(
		policyGroups,
		selectedPolicyGroupId ?? team?.policy_group_id ?? null,
	);
	const currentPolicyGroup =
		team?.policy_group_id != null
			? (policyGroups.find((group) => group.id === team.policy_group_id) ??
				null)
			: null;
	const selectedPolicyGroup =
		selectedPolicyGroupId != null
			? (policyGroups.find((group) => group.id === selectedPolicyGroupId) ??
				null)
			: null;
	const policyGroupUnavailable =
		!policyGroupsLoading && policyGroupOptions.length === 0;
	const assignedPolicyGroupIsInvalid =
		!policyGroupsLoading &&
		team?.policy_group_id != null &&
		(currentPolicyGroup === null ||
			!currentPolicyGroup.is_enabled ||
			currentPolicyGroup.items.length === 0);
	const canMutateTeam = team != null && team.archived_at == null;
	const hasChanges =
		canMutateTeam &&
		(name.trim() !== team.name ||
			(description.trim() || "") !== team.description ||
			selectedPolicyGroupId !== (team.policy_group_id ?? null));
	const hasMemberFilters =
		memberKeyword.length > 0 ||
		memberRoleFilter !== "__all__" ||
		memberStatusFilter !== "__all__";
	const memberTotalPages = Math.max(
		1,
		Math.ceil(memberTotal / ADMIN_TEAM_DETAIL_MEMBER_PAGE_SIZE),
	);
	const memberCurrentPage =
		Math.floor(memberOffset / ADMIN_TEAM_DETAIL_MEMBER_PAGE_SIZE) + 1;
	const prevMemberPageDisabled = memberOffset === 0;
	const nextMemberPageDisabled =
		memberOffset + ADMIN_TEAM_DETAIL_MEMBER_PAGE_SIZE >= memberTotal;
	const auditTotalPages = Math.max(
		1,
		Math.ceil(auditTotal / ADMIN_TEAM_DETAIL_AUDIT_PAGE_SIZE),
	);
	const auditCurrentPage =
		Math.floor(auditOffset / ADMIN_TEAM_DETAIL_AUDIT_PAGE_SIZE) + 1;
	const prevAuditPageDisabled = auditOffset === 0;
	const nextAuditPageDisabled =
		auditOffset + ADMIN_TEAM_DETAIL_AUDIT_PAGE_SIZE >= auditTotal;

	useEffect(() => {
		if (memberOffset < memberTotal || memberTotal === 0) {
			return;
		}

		setMemberOffset(
			Math.max(0, (memberTotalPages - 1) * ADMIN_TEAM_DETAIL_MEMBER_PAGE_SIZE),
		);
	}, [memberOffset, memberTotal, memberTotalPages]);

	const handleSave = async () => {
		if (!team || !canMutateTeam) {
			return;
		}

		const nextName = name.trim();
		const nextPolicyGroupId = Number(policyGroupId);
		if (!nextName || !Number.isFinite(nextPolicyGroupId)) {
			return;
		}

		try {
			setSaving(true);
			overviewSyncAllowedRef.current = true;
			await adminTeamService.update(team.id, {
				name: nextName,
				description: description.trim() || undefined,
				policy_group_id: nextPolicyGroupId,
			});
			await Promise.all([
				loadTeamDetail(team.id),
				loadAuditEntries(team.id),
				onListChange(),
			]);
			toast.success(t("team_updated"));
		} catch (error) {
			handleApiError(error);
		} finally {
			setSaving(false);
		}
	};

	const handleArchive = async () => {
		if (!team || !canMutateTeam) {
			return;
		}

		try {
			setArchiving(true);
			overviewSyncAllowedRef.current = true;
			await adminTeamService.delete(team.id);
			await Promise.all([
				loadTeamDetail(team.id),
				loadAuditEntries(team.id),
				onListChange(),
			]);
			archiveDialogProps.onOpenChange(false);
			toast.success(t("team_deleted"));
		} catch (error) {
			handleApiError(error);
		} finally {
			setArchiving(false);
		}
	};

	const handleRestore = async () => {
		if (!team || team.archived_at == null) {
			return;
		}

		try {
			setRestoring(true);
			overviewSyncAllowedRef.current = true;
			await adminTeamService.restore(team.id);
			await Promise.all([
				loadTeamDetail(team.id),
				loadAuditEntries(team.id),
				onListChange(),
			]);
			toast.success(t("team_restored"));
		} catch (error) {
			handleApiError(error);
		} finally {
			setRestoring(false);
		}
	};

	const handleAddMember = async (event: FormEvent<HTMLFormElement>) => {
		event.preventDefault();
		if (teamId == null || !canMutateTeam) {
			return;
		}

		const identifier = memberIdentifier.trim();
		if (!identifier) {
			return;
		}

		try {
			setMemberMutating(true);
			await adminTeamService.addMember(teamId, {
				identifier,
				role: memberRole,
			});
			setMemberIdentifier("");
			setMemberRole("member");
			setMemberOffset(0);
			await Promise.all([
				loadTeamDetail(teamId),
				loadMembers(teamId, 0, memberFilters, memberSortBy, memberSortOrder),
				loadAuditEntries(teamId),
				onListChange(),
			]);
			toast.success(t("settings:settings_team_member_added"));
		} catch (error) {
			handleApiError(error);
		} finally {
			setMemberMutating(false);
		}
	};

	const handleUpdateMemberRole = async (
		memberUserId: number,
		role: TeamMemberRole,
	) => {
		if (teamId == null || !canMutateTeam) {
			return;
		}

		try {
			setMemberMutating(true);
			await adminTeamService.updateMember(teamId, memberUserId, { role });
			await Promise.all([
				loadTeamDetail(teamId),
				loadMembers(
					teamId,
					memberOffset,
					memberFilters,
					memberSortBy,
					memberSortOrder,
				),
				loadAuditEntries(teamId),
				onListChange(),
			]);
			toast.success(t("settings:settings_team_member_role_updated"));
		} catch (error) {
			handleApiError(error);
		} finally {
			setMemberMutating(false);
		}
	};

	const handleRemoveMember = async (memberUserId: number) => {
		if (teamId == null || !canMutateTeam) {
			return;
		}

		try {
			setMemberMutating(true);
			await adminTeamService.removeMember(teamId, memberUserId);
			await Promise.all([
				loadTeamDetail(teamId),
				loadMembers(
					teamId,
					memberOffset,
					memberFilters,
					memberSortBy,
					memberSortOrder,
				),
				loadAuditEntries(teamId),
				onListChange(),
			]);
			toast.success(t("settings:settings_team_member_removed"));
		} catch (error) {
			handleApiError(error);
		} finally {
			setMemberMutating(false);
		}
	};

	const {
		confirmId: removeMemberId,
		requestConfirm: requestRemoveConfirm,
		dialogProps: removeDialogProps,
	} = useConfirmDialog(handleRemoveMember);
	const {
		requestConfirm: requestArchiveConfirm,
		dialogProps: archiveDialogProps,
	} = useConfirmDialog<true>(handleArchive);

	useEffect(() => {
		if (!open || teamId == null) {
			overviewSyncAllowedRef.current = true;
			setArchiveConfirmValue("");
			archiveDialogProps.onOpenChange(false);
			setArchiving(false);
			setAuditOffset(0);
			setDescription("");
			setMemberIdentifier("");
			setMemberMutating(false);
			setMemberOffset(0);
			setMemberQuery("");
			setMemberRole("member");
			setMemberRoleFilter("__all__");
			setMemberSortBy("role");
			setMemberSortOrder("asc");
			setMemberStatusFilter("__all__");
			setName("");
			setPolicyGroupId("");
			setRestoring(false);
			setSaving(false);
			resetDialogTab();
			return;
		}

		overviewSyncAllowedRef.current = true;
		setAuditOffset(0);
		setMemberOffset(0);
		setMemberSortBy("role");
		setMemberSortOrder("asc");
		resetDialogTab();
	}, [archiveDialogProps.onOpenChange, open, resetDialogTab, teamId]);

	const removeMember =
		members.find((member) => member.user_id === removeMemberId) ?? null;

	if (teamId == null) {
		return null;
	}

	const handleDialogOpenChange = (nextOpen: boolean) => {
		if (!nextOpen) {
			archiveDialogProps.onOpenChange(false);
		}
		onOpenChange(nextOpen);
	};
	const handleArchiveDialogOpenChange = (nextOpen: boolean) => {
		if (nextOpen) {
			requestArchiveConfirm(true);
			return;
		}
		archiveDialogProps.onOpenChange(false);
	};

	const handleContentScroll = () => {
		if (teamId == null || contentRef.current == null) {
			return;
		}

		adminTeamDetailContentScrollPositions.set(
			teamId,
			contentRef.current.scrollTop,
		);
	};

	const handleSidebarScroll = () => {
		if (teamId == null || sidebarRef.current == null) {
			return;
		}

		adminTeamDetailSidebarScrollPositions.set(
			teamId,
			sidebarRef.current.scrollTop,
		);
	};

	const handleMemberSortChange = (
		nextSortBy: AdminTeamMemberSortBy,
		nextOrder: SortOrder,
	) => {
		setMemberSortBy(nextSortBy);
		setMemberSortOrder(nextOrder);
		setMemberOffset(0);
	};

	const overviewSection = (
		<AdminTeamDetailOverviewSection
			archiving={archiving}
			assignedPolicyGroupIsInvalid={assignedPolicyGroupIsInvalid}
			canMutateTeam={canMutateTeam}
			description={description}
			detailLoading={detailLoading}
			hasChanges={hasChanges}
			name={name}
			onDescriptionChange={handleDescriptionChange}
			onNameChange={handleNameChange}
			onPolicyGroupChange={handlePolicyGroupChange}
			onRefreshPolicyGroups={onRefreshPolicyGroups}
			onSave={handleSave}
			policyGroupId={policyGroupId}
			policyGroupOptions={policyGroupOptions}
			policyGroupUnavailable={policyGroupUnavailable}
			policyGroupsLoading={policyGroupsLoading}
			restoring={restoring}
			saving={saving}
			team={team}
		/>
	);

	const membersSection = (
		<AdminTeamDetailMembersSection
			canMutateTeam={canMutateTeam}
			hasMemberFilters={hasMemberFilters}
			managerCount={managerCount}
			memberCurrentPage={memberCurrentPage}
			memberIdentifier={memberIdentifier}
			memberLoading={memberLoading}
			memberMutating={memberMutating}
			memberOffset={memberOffset}
			memberQuery={memberQuery}
			memberRole={memberRole}
			memberRoleFilter={memberRoleFilter}
			memberSortBy={memberSortBy}
			memberSortOrder={memberSortOrder}
			memberStatusFilter={memberStatusFilter}
			memberTotal={memberTotal}
			memberTotalPages={memberTotalPages}
			members={members}
			nextMemberPageDisabled={nextMemberPageDisabled}
			ownerCount={ownerCount}
			prevMemberPageDisabled={prevMemberPageDisabled}
			requestRemoveConfirm={requestRemoveConfirm}
			roleFilterOptions={roleFilterOptions}
			roleLabel={roleLabel}
			roleOptions={roleOptions}
			setMemberIdentifier={setMemberIdentifier}
			setMemberOffset={setMemberOffset}
			setMemberQuery={setMemberQuery}
			setMemberRole={setMemberRole}
			setMemberRoleFilter={setMemberRoleFilter}
			setMemberStatusFilter={setMemberStatusFilter}
			statusFilterOptions={statusFilterOptions}
			team={team}
			onAddMember={handleAddMember}
			onMemberSortChange={handleMemberSortChange}
			onUpdateMemberRole={handleUpdateMemberRole}
		/>
	);

	const auditSection = (
		<AdminTeamDetailAuditSection
			auditCurrentPage={auditCurrentPage}
			auditEntries={auditEntries}
			auditLoading={auditLoading}
			auditOffset={auditOffset}
			auditTotal={auditTotal}
			auditTotalPages={auditTotalPages}
			nextAuditPageDisabled={nextAuditPageDisabled}
			prevAuditPageDisabled={prevAuditPageDisabled}
			roleLabel={roleLabel}
			setAuditOffset={setAuditOffset}
		/>
	);

	const dangerSection = (
		<AdminTeamDetailDangerSection
			archiveConfirmValue={archiveConfirmValue}
			archiving={archiving}
			detailLoading={detailLoading}
			managerCount={managerCount}
			ownerCount={ownerCount}
			restoring={restoring}
			setArchiveConfirmValue={setArchiveConfirmValue}
			setArchiveDialogOpen={handleArchiveDialogOpenChange}
			team={team}
			onRestore={handleRestore}
		/>
	);

	return (
		<>
			<AdminTeamDetailShell
				auditSection={auditSection}
				contentRef={contentRef}
				currentPolicyGroupName={currentPolicyGroup?.name ?? null}
				currentTab={currentTab}
				dangerSection={dangerSection}
				isPageLayout={isPageLayout}
				membersSection={membersSection}
				onContentScroll={handleContentScroll}
				onOpenChange={handleDialogOpenChange}
				onPageBack={() => onOpenChange(false)}
				onSidebarScroll={handleSidebarScroll}
				onTabChange={handleTabChange}
				open={open}
				overviewSection={overviewSection}
				ownerCount={ownerCount}
				managerCount={managerCount}
				panelAnimationClass={panelAnimationClass}
				quota={quota}
				selectedPolicyGroupName={selectedPolicyGroup?.name ?? null}
				sidebarRef={sidebarRef}
				team={team}
				usagePercentage={usagePercentage}
				used={used}
			/>

			<ConfirmDialog
				{...removeDialogProps}
				title={t("settings:settings_team_remove_member")}
				description={
					removeMember
						? `${t("settings:settings_team_remove_member_desc")} ${getUserDisplayName(removeMember.user)}`
						: t("settings:settings_team_remove_member_desc")
				}
				confirmLabel={t("settings:settings_team_remove_member")}
				variant="destructive"
			/>

			<ConfirmDialog
				{...archiveDialogProps}
				title={team ? `${t("delete_team")} "${team.name}"?` : t("delete_team")}
				description={t("archive_team_desc")}
				confirmLabel={t("core:delete")}
				variant="destructive"
			/>
		</>
	);
}
