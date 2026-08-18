import type { FormEvent } from "react";
import {
	TEAM_MANAGE_AUDIT_PAGE_SIZE,
	TEAM_MANAGE_MEMBER_PAGE_SIZE,
} from "@/components/settings/team-manage-detail/teamManageDetailState";
import type {
	TeamAuditEntryInfo,
	TeamInfo,
	TeamMemberInfo,
	TeamMemberRole,
	UserStatus,
} from "@/types/api";
import {
	TeamManageAuditSection,
	TeamManageDangerSection,
	TeamManageMembersSection,
	TeamManageOverviewSection,
	TeamManageWebdavSection,
} from "./TeamManageSections";

interface UseTeamManageSectionsArgs {
	archiveConfirmValue: string;
	auditCurrentPage: number;
	auditEntries: TeamAuditEntryInfo[];
	auditLoading: boolean;
	auditOffset: number;
	auditTotal: number;
	auditTotalPages: number;
	canArchiveTeam: boolean;
	canAssignOwner: boolean;
	canManageTeam: boolean;
	currentUserId: number | null;
	detailLoading: boolean;
	displayTeam: TeamInfo | null;
	handleArchiveTeam: () => Promise<void>;
	handleRemoveMember: (memberUserId: number) => Promise<void>;
	handleUpdateMemberRole: (
		memberUserId: number,
		role: TeamMemberRole,
	) => void | Promise<void>;
	hasMemberFilters: boolean;
	managerCount: number;
	memberCurrentPage: number;
	memberIdentifier: string;
	memberLoading: boolean;
	memberRole: TeamMemberRole;
	memberRoleFilter: "__all__" | TeamMemberRole;
	memberStatusFilter: "__all__" | UserStatus;
	memberOffset: number;
	memberQuery: string;
	memberTotal: number;
	memberTotalPages: number;
	members: TeamMemberInfo[];
	mutating: boolean;
	nextAuditPageDisabled: boolean;
	nextMemberPageDisabled: boolean;
	onAddMember: (event: FormEvent<HTMLFormElement>) => void;
	onUpdateTeam: (event: FormEvent<HTMLFormElement>) => void;
	ownerCount: number;
	prevAuditPageDisabled: boolean;
	prevMemberPageDisabled: boolean;
	roleFilterOptions: ReadonlyArray<{
		label: string;
		value: "__all__" | TeamMemberRole;
	}>;
	roleLabel: (role: TeamMemberRole) => string;
	roleOptions: TeamMemberRole[];
	setArchiveConfirmValue: (value: string) => void;
	setAuditOffset: (offset: number) => void;
	setMemberIdentifier: (value: string) => void;
	setMemberOffset: (offset: number) => void;
	setMemberQuery: (value: string) => void;
	setMemberRole: (value: TeamMemberRole) => void;
	setMemberRoleFilter: (value: "__all__" | TeamMemberRole) => void;
	setMemberStatusFilter: (value: "__all__" | UserStatus) => void;
	setTeamDescription: (description: string) => void;
	setTeamName: (name: string) => void;
	statusFilterOptions: ReadonlyArray<{
		label: string;
		value: "__all__" | UserStatus;
	}>;
	teamDescription: string;
	teamId: number;
	teamName: string;
	viewerRole: TeamMemberRole | null;
	webdavPrefix: string;
}

export function buildTeamManageSections({
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
	memberOffset,
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
	onAddMember,
	onUpdateTeam,
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
}: UseTeamManageSectionsArgs) {
	const overviewSection = (
		<TeamManageOverviewSection
			canManageTeam={canManageTeam}
			detailLoading={detailLoading}
			mutating={mutating}
			onDescriptionChange={setTeamDescription}
			onSubmit={(event) => void onUpdateTeam(event)}
			onTeamNameChange={setTeamName}
			team={displayTeam}
			teamDescription={teamDescription}
			teamName={teamName}
		/>
	);
	const membersSection = (
		<TeamManageMembersSection
			canAssignOwner={canAssignOwner}
			canManageTeam={canManageTeam}
			currentUserId={currentUserId}
			hasMemberFilters={hasMemberFilters}
			managerCount={managerCount}
			memberCurrentPage={memberCurrentPage}
			memberIdentifier={memberIdentifier}
			memberLoading={memberLoading}
			memberOffset={memberOffset}
			memberPageSize={TEAM_MANAGE_MEMBER_PAGE_SIZE}
			memberQuery={memberQuery}
			memberRole={memberRole}
			memberRoleFilter={memberRoleFilter}
			memberStatusFilter={memberStatusFilter}
			memberTotal={memberTotal}
			memberTotalPages={memberTotalPages}
			members={members}
			mutating={mutating}
			nextMemberPageDisabled={nextMemberPageDisabled}
			onAddMember={(event) => void onAddMember(event)}
			onRemoveMember={handleRemoveMember}
			onUpdateMemberRole={handleUpdateMemberRole}
			ownerCount={ownerCount}
			prevMemberPageDisabled={prevMemberPageDisabled}
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
			team={displayTeam}
			viewerRole={viewerRole}
		/>
	);
	const auditSection = canManageTeam ? (
		<TeamManageAuditSection
			auditCurrentPage={auditCurrentPage}
			auditEntries={auditEntries}
			auditLoading={auditLoading}
			auditOffset={auditOffset}
			auditPageSize={TEAM_MANAGE_AUDIT_PAGE_SIZE}
			auditTotal={auditTotal}
			auditTotalPages={auditTotalPages}
			nextAuditPageDisabled={nextAuditPageDisabled}
			prevAuditPageDisabled={prevAuditPageDisabled}
			roleLabel={roleLabel}
			setAuditOffset={setAuditOffset}
		/>
	) : null;
	const dangerSection = canArchiveTeam ? (
		<TeamManageDangerSection
			archiveConfirmValue={archiveConfirmValue}
			managerCount={managerCount}
			mutating={mutating}
			ownerCount={ownerCount}
			setArchiveConfirmValue={setArchiveConfirmValue}
			team={displayTeam}
			onArchive={handleArchiveTeam}
		/>
	) : null;
	const webdavSection = (
		<TeamManageWebdavSection
			key={teamId}
			canManageTeam={canManageTeam}
			currentUserId={currentUserId}
			teamId={teamId}
			webdavPrefix={webdavPrefix}
		/>
	);

	return {
		auditSection,
		dangerSection,
		membersSection,
		overviewSection,
		webdavSection,
	};
}
