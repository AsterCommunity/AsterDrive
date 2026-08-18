import { useEffect } from "react";
import { useTranslation } from "react-i18next";
import { useNavigate } from "react-router-dom";
import { handleApiError } from "@/hooks/useApiError";
import { normalizeWebdavPrefix } from "@/lib/webdav";
import { webdavAccountService } from "@/services/webdavAccountService";
import type { TeamInfo, TeamMemberRole } from "@/types/api";
import { TeamManageShell } from "./TeamManageShell";
import type { TeamManageTab } from "./types";
import { useTeamManageActions } from "./useTeamManageActions";
import { useTeamManageData } from "./useTeamManageData";
import { useTeamManageLocalState } from "./useTeamManageLocalState";
import { useTeamManageScrollRestoration } from "./useTeamManageScrollRestoration";
import { buildTeamManageSections } from "./useTeamManageSections";
import { useTeamManageTabs } from "./useTeamManageTabs";
import { useTeamManageViewModel } from "./useTeamManageViewModel";

interface TeamManageDetailProps {
	currentUserId: number | null;
	onExit: () => void;
	onPageTabChange: (
		tab: TeamManageTab,
		options?: { replace?: boolean },
	) => void;
	onTeamsReload: () => Promise<void>;
	pageTab: TeamManageTab;
	teamId: number;
	teamSummary: TeamInfo | null;
}

export function TeamManageDetail({
	currentUserId,
	onExit,
	onPageTabChange,
	onTeamsReload,
	pageTab,
	teamId,
	teamSummary,
}: TeamManageDetailProps) {
	const { t } = useTranslation(["core", "settings"]);
	const navigate = useNavigate();
	const localState = useTeamManageLocalState(teamId);
	const {
		archiveConfirmValue,
		auditOffset,
		memberIdentifier,
		memberOffset,
		memberQuery,
		memberRole,
		memberRoleFilter,
		memberStatusFilter,
		setArchiveConfirmValue,
		setAuditOffset,
		setMemberIdentifier,
		setMemberOffset,
		setMemberQuery,
		setMemberRole,
		setMemberRoleFilter,
		setMemberStatusFilter,
		setTeamDraft,
		setWebdavPrefix,
		teamDraft,
		webdavPrefix,
	} = localState;
	const roleLabel = (role: TeamMemberRole) =>
		t(`settings:settings_team_role_${role}`);
	const viewModel = useTeamManageViewModel({
		activeTeamId: teamId,
		auditOffset,
		auditTotal: 0,
		canAssignOwner: false,
		displayTeam: null,
		memberOffset,
		memberQuery,
		memberRoleFilter,
		memberStatusFilter,
		memberTotal: 0,
		roleLabel,
		t,
		teamDraft,
	});
	const {
		auditEntries,
		auditLoading,
		auditTotal,
		canArchiveTeam,
		canAssignOwner,
		canManageTeam,
		detailLoading,
		detailRequestStarted,
		displayTeam,
		loadAuditEntries,
		loadMembers,
		loadTeamDetail,
		managerCount,
		memberLoading,
		memberTotal,
		members,
		ownerCount,
		teamDetail,
		viewerRole,
	} = useTeamManageData({
		auditOffset,
		memberFilters: viewModel.memberFilters,
		memberOffset,
		teamId,
		teamSummary,
	});
	const {
		auditCurrentPage,
		auditTotalPages,
		hasMemberFilters,
		memberCurrentPage,
		memberTotalPages,
		nextAuditPageDisabled,
		nextMemberPageDisabled,
		prevAuditPageDisabled,
		prevMemberPageDisabled,
		quota,
		roleFilterOptions,
		roleOptions,
		safeMemberOffset,
		statusFilterOptions,
		teamBaseDescription,
		teamBaseName,
		teamDescription,
		teamName,
		usagePercentage,
		used,
	} = useTeamManageViewModel({
		activeTeamId: teamId,
		auditOffset,
		auditTotal,
		canAssignOwner,
		displayTeam,
		memberOffset,
		memberQuery,
		memberRoleFilter,
		memberStatusFilter,
		memberTotal,
		roleLabel,
		t,
		teamDraft,
	});
	const { contentRef, handleContentScroll, handleSidebarScroll, sidebarRef } =
		useTeamManageScrollRestoration({
			pageTab,
			teamId,
		});
	const { currentTab, handleTabChange, panelAnimationClass } =
		useTeamManageTabs({
			canArchiveTeam,
			canManageTeam,
			detailLoading,
			detailRequestStarted,
			onPageTabChange,
			pageTab,
		});
	const setTeamName = (name: string) => {
		setTeamDraft({
			baseDescription: teamBaseDescription,
			baseName: teamBaseName,
			description: teamDescription,
			name,
			teamId,
		});
	};
	const setTeamDescription = (description: string) => {
		setTeamDraft({
			baseDescription: teamBaseDescription,
			baseName: teamBaseName,
			description,
			name: teamName,
			teamId,
		});
	};

	useEffect(() => {
		let cancelled = false;
		void webdavAccountService
			.settings()
			.then((settings) => {
				if (!cancelled) {
					setWebdavPrefix(normalizeWebdavPrefix(settings.prefix));
				}
			})
			.catch(handleApiError);

		return () => {
			cancelled = true;
		};
	}, [setWebdavPrefix]);

	const {
		handleAddMember,
		handleArchiveTeam,
		handleRemoveMember,
		handleUpdateMemberRole,
		handleUpdateTeam,
		mutating,
	} = useTeamManageActions({
		canArchiveTeam,
		canManageTeam,
		currentUserId,
		loadAuditEntries,
		loadMembers,
		loadTeamDetail,
		onExit,
		onTeamsReload,
		safeMemberOffset,
		setMemberIdentifier,
		setMemberOffset,
		setMemberRole,
		teamDetail,
		teamId,
	});

	const {
		auditSection,
		dangerSection,
		membersSection,
		overviewSection,
		webdavSection,
	} = buildTeamManageSections({
		archiveConfirmValue,
		auditCurrentPage,
		auditEntries,
		auditLoading,
		auditOffset,
		auditTotal,
		auditTotalPages,
		canArchiveTeam,
		canAssignOwner,
		canManageTeam,
		currentUserId,
		detailLoading,
		displayTeam,
		handleArchiveTeam,
		handleRemoveMember,
		handleUpdateMemberRole,
		hasMemberFilters,
		managerCount,
		memberCurrentPage,
		memberIdentifier,
		memberLoading,
		memberOffset: safeMemberOffset,
		memberQuery,
		memberRole,
		memberRoleFilter,
		memberStatusFilter,
		memberTotal,
		memberTotalPages,
		members,
		mutating,
		nextAuditPageDisabled,
		nextMemberPageDisabled,
		onAddMember: (event) => {
			event.preventDefault();
			void handleAddMember(memberIdentifier, memberRole);
		},
		onUpdateTeam: (event) => {
			event.preventDefault();
			void handleUpdateTeam(teamName, teamDescription);
		},
		ownerCount,
		prevAuditPageDisabled,
		prevMemberPageDisabled,
		roleFilterOptions,
		roleLabel,
		roleOptions,
		setArchiveConfirmValue,
		setAuditOffset,
		setMemberIdentifier,
		setMemberOffset,
		setMemberQuery,
		setMemberRole,
		setMemberRoleFilter,
		setMemberStatusFilter,
		setTeamDescription,
		setTeamName,
		statusFilterOptions,
		teamDescription,
		teamId,
		teamName,
		viewerRole,
		webdavPrefix,
	});

	return (
		<TeamManageShell
			auditSection={auditSection}
			canArchiveTeam={canArchiveTeam}
			canManageTeam={canManageTeam}
			contentRef={contentRef}
			currentTab={currentTab}
			dangerSection={dangerSection}
			managerCount={managerCount}
			membersSection={membersSection}
			onContentScroll={handleContentScroll}
			onOpenWorkspace={() =>
				navigate(`/teams/${teamId}`, { viewTransition: false })
			}
			onPageBack={onExit}
			onSidebarScroll={handleSidebarScroll}
			onTabChange={handleTabChange}
			overviewSection={overviewSection}
			ownerCount={ownerCount}
			panelAnimationClass={panelAnimationClass}
			quota={quota}
			roleLabel={roleLabel}
			sidebarRef={sidebarRef}
			team={displayTeam}
			usagePercentage={usagePercentage}
			used={used}
			viewerRole={viewerRole}
			webdavSection={webdavSection}
		/>
	);
}
