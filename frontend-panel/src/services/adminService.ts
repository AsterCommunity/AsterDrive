import { withQuery } from "@/lib/queryParams";
import type {
	ActionMessageResp,
	AddTeamMemberRequest,
	AdminConfigListQuery,
	AdminCreateTeamRequest,
	AdminExternalAuthProviderInfo,
	AdminExternalAuthProviderKindInfo,
	AdminExternalAuthProviderListQuery,
	AdminExternalAuthProviderPage,
	AdminFileBlobDetail,
	AdminFileBlobListQuery,
	AdminFileBlobPage,
	AdminFileDetail,
	AdminFileListQuery,
	AdminFilePage,
	AdminLockListQuery,
	AdminOverview,
	AdminOverviewQuery,
	AdminPolicyGroupListQuery,
	AdminPolicyListQuery,
	AdminRemoteNodeListQuery,
	AdminShareListQuery,
	AdminSharePage,
	AdminTaskCleanupRequest,
	AdminTaskListQuery,
	AdminTeamAuditLogListQuery,
	AdminTeamInfo,
	AdminTeamListQuery,
	AdminTeamMemberListQuery,
	AdminTeamPage,
	AdminUpdateTeamRequest,
	AdminUserInvitationInfo,
	AdminUserInvitationListQuery,
	AdminUserInvitationPage,
	AdminUserListQuery,
	ConfigActionType,
	ConfigSchemaItem,
	CreateBlobMaintenanceTaskRequest,
	CreateExternalAuthProviderInput,
	CreatePolicyGroupRequest,
	CreatePolicyRequest,
	CreateRemoteNodeRequest,
	CreateStoragePolicyMigrationRequest,
	CreateUserInvitationRequest,
	CreateUserReq,
	CreateUserResponse,
	DeletePolicyQuery,
	DryRunStoragePolicyMigrationRequest,
	ExecuteConfigActionRequest,
	ExecuteConfigActionResponse,
	ExecuteDraftStoragePolicyActionRequest,
	ExecuteSavedStoragePolicyActionRequest,
	ExternalAuthProviderTestParamsInput,
	ExternalAuthProviderTestResult,
	FolderInfo,
	LockPage,
	ManagedIngressDriverDescriptor,
	MigratePolicyGroupAssignmentsRequest,
	PolicyGroupAssignmentMigrationResult,
	PromoteS3CompatiblePolicyDriverRequest,
	RemoteCreateIngressProfileRequest,
	RemoteEnrollmentCommandInfo,
	RemoteIngressProfileInfo,
	RemoteNodeInfo,
	RemoteNodePage,
	RemoteStorageCapabilities,
	RemoteUpdateIngressProfileRequest,
	RemovedCountResponse,
	ResetUserPasswordRequest,
	ShareInfo,
	StartStorageAuthorizationRequest,
	StorageAuthorizationStartResponse,
	StorageConnectorDescriptor,
	StorageCredentialProvider,
	StorageCredentialProviderInfo,
	StoragePolicy,
	StoragePolicyActionResult,
	StoragePolicyCapacityInfo,
	StoragePolicyConnectionTestResult,
	StoragePolicyCredentialInfo,
	StoragePolicyCredentialValidationResult,
	StoragePolicyGroup,
	StoragePolicyGroupPage,
	StoragePolicyMigrationDryRun,
	StoragePolicyPage,
	SystemConfig,
	SystemConfigPage,
	SystemConfigVisibility,
	SystemInfoResponse,
	TaskInfo,
	TaskPage,
	TeamAuditPage,
	TeamMemberPage,
	TemplateVariableGroup,
	TestPolicyParamsRequest,
	TestRemoteNodeParamsReq,
	UpdateExternalAuthProviderInput,
	UpdatePolicyGroupRequest,
	UpdatePolicyRequest,
	UpdateRemoteNodeRequest,
	UpdateTeamMemberRequest,
	UpdateUserRequest,
	UserInfo,
	UserPage,
} from "@/types/api";
import { api } from "./http";

export interface SetFolderPolicyRequest {
	policy_id: number | null;
}

// The admin PATCH endpoint rejects `policy_group_id: null`, and current callers
// only support assigning a group or leaving it unchanged. Strip accidental nulls
// here so broader callers cannot request an unsupported clear operation.
function sanitizeUpdateUserRequest(data: UpdateUserRequest): UpdateUserRequest {
	const rawData = data as UpdateUserRequest & {
		policy_group_id?: number | null;
	};
	if (rawData.policy_group_id != null) {
		return data;
	}

	const { policy_group_id: _policyGroupId, ...payload } = rawData;
	return payload;
}

export const adminOverviewService = {
	get: (params?: AdminOverviewQuery) =>
		api.get<AdminOverview>(
			withQuery("/admin/overview", {
				days: params?.days,
				timezone: params?.timezone,
				event_limit: params?.event_limit,
			}),
		),
};

export const adminSystemService = {
	getInfo: () => api.get<SystemInfoResponse>("/admin/system-info"),
};

// --- Users ---

export const adminUserService = {
	list: (params?: AdminUserListQuery) =>
		api.get<UserPage>(
			withQuery("/admin/users", {
				limit: params?.limit,
				offset: params?.offset,
				keyword: params?.keyword,
				role: params?.role,
				status: params?.status,
				sort_by: params?.sort_by,
				sort_order: params?.sort_order,
			}),
		),

	get: (id: number) => api.get<UserInfo>(`/admin/users/${id}`),

	create: (data: CreateUserReq) =>
		api.post<CreateUserResponse>("/admin/users", data),

	update: (id: number, data: UpdateUserRequest) =>
		api.patch<UserInfo>(`/admin/users/${id}`, sanitizeUpdateUserRequest(data)),

	resetPassword: (id: number, data: ResetUserPasswordRequest) =>
		api.put<void>(`/admin/users/${id}/password`, data),

	revokeSessions: (id: number) =>
		api.post<void>(`/admin/users/${id}/sessions/revoke`),

	resetMfa: (id: number) => api.delete<void>(`/admin/users/${id}/mfa`),

	delete: (id: number) => api.delete<void>(`/admin/users/${id}`),

	listInvitations: (params?: AdminUserInvitationListQuery) =>
		api.get<AdminUserInvitationPage>(
			withQuery("/admin/users/invitations", {
				limit: params?.limit,
				offset: params?.offset,
			}),
		),

	createInvitation: (data: CreateUserInvitationRequest) =>
		api.post<AdminUserInvitationInfo>("/admin/users/invitations", data),

	revokeInvitation: (id: number) =>
		api.post<AdminUserInvitationInfo>(`/admin/users/invitations/${id}/revoke`),
};

// --- Teams ---

export const adminTeamService = {
	list: (params?: AdminTeamListQuery) =>
		api.get<AdminTeamPage>(
			withQuery("/admin/teams", {
				limit: params?.limit,
				offset: params?.offset,
				keyword: params?.keyword,
				archived: params?.archived,
				sort_by: params?.sort_by,
				sort_order: params?.sort_order,
			}),
		),

	get: (id: number) => api.get<AdminTeamInfo>(`/admin/teams/${id}`),

	create: (data: AdminCreateTeamRequest) =>
		api.post<AdminTeamInfo>("/admin/teams", data),

	update: (id: number, data: AdminUpdateTeamRequest) =>
		api.patch<AdminTeamInfo>(`/admin/teams/${id}`, data),

	delete: (id: number) => api.delete<void>(`/admin/teams/${id}`),
	restore: (id: number) =>
		api.post<AdminTeamInfo>(`/admin/teams/${id}/restore`),
	listAuditLogs: (id: number, params: AdminTeamAuditLogListQuery = {}) => {
		const { limit, offset, ...filters } = params;

		return api.get<TeamAuditPage>(
			withQuery(`/admin/teams/${id}/audit-logs`, {
				limit,
				offset,
				...filters,
			}),
		);
	},
	listMembers: (id: number, params: AdminTeamMemberListQuery = {}) => {
		const { limit, offset, sort_by, sort_order, ...filters } = params;

		return api.get<TeamMemberPage>(
			withQuery(`/admin/teams/${id}/members`, {
				limit,
				offset,
				sort_by,
				sort_order,
				...filters,
			}),
		);
	},
	addMember: (id: number, data: AddTeamMemberRequest) =>
		api.post<TeamMemberPage["items"][number]>(
			`/admin/teams/${id}/members`,
			data,
		),
	updateMember: (
		id: number,
		memberUserId: number,
		data: UpdateTeamMemberRequest,
	) =>
		api.patch<TeamMemberPage["items"][number]>(
			`/admin/teams/${id}/members/${memberUserId}`,
			data,
		),
	removeMember: (id: number, memberUserId: number) =>
		api.delete<void>(`/admin/teams/${id}/members/${memberUserId}`),
};

// --- Policies ---

export const adminPolicyService = {
	list: (params?: AdminPolicyListQuery) =>
		api.get<StoragePolicyPage>(
			withQuery("/admin/policies", {
				limit: params?.limit,
				offset: params?.offset,
				sort_by: params?.sort_by,
				sort_order: params?.sort_order,
			}),
		),

	listAll: async (pageSize = 100) => {
		if (!Number.isInteger(pageSize) || pageSize <= 0) {
			throw new Error("pageSize must be a positive integer");
		}

		const allPolicies: StoragePolicy[] = [];
		let offset = 0;
		let total = 0;
		let pageCount = 0;
		let maxPages = Number.POSITIVE_INFINITY;

		do {
			pageCount += 1;
			if (pageCount > maxPages) {
				throw new Error("pagination exceeded max iterations");
			}

			const previousOffset = offset;
			const previousCount = allPolicies.length;
			const page = await adminPolicyService.list({
				limit: pageSize,
				offset,
				sort_by: "id",
				sort_order: "asc",
			});
			allPolicies.push(...page.items);
			total = page.total;
			maxPages = Math.max(1, Math.ceil(total / pageSize)) + 2;
			offset += page.items.length;
			if (page.items.length === 0) {
				if (allPolicies.length < total) {
					throw new Error("incomplete pages from adminPolicyService.list");
				}
				break;
			}
			if (offset <= previousOffset || allPolicies.length <= previousCount) {
				throw new Error("pagination did not make progress");
			}
		} while (allPolicies.length < total);

		return allPolicies;
	},

	get: (id: number) => api.get<StoragePolicy>(`/admin/policies/${id}`),

	getCapacity: (id: number) =>
		api.get<StoragePolicyCapacityInfo>(`/admin/policies/${id}/capacity`),

	listStorageDriverDescriptors: () =>
		api.get<StorageConnectorDescriptor[]>("/admin/policies/storage-drivers"),

	create: (data: CreatePolicyRequest) =>
		api.post<StoragePolicy>("/admin/policies", data),

	update: (id: number, data: UpdatePolicyRequest) =>
		api.patch<StoragePolicy>(`/admin/policies/${id}`, data),

	delete: (id: number, params?: DeletePolicyQuery) =>
		api.delete<void>(
			withQuery(`/admin/policies/${id}`, {
				force: params?.force,
			}),
		),

	testConnection: (id: number) =>
		api.post<StoragePolicyConnectionTestResult>(`/admin/policies/${id}/test`),

	testParams: (data: TestPolicyParamsRequest) =>
		api.post<StoragePolicyConnectionTestResult>("/admin/policies/test", data),

	executeDraftPolicyAction: (data: ExecuteDraftStoragePolicyActionRequest) =>
		api.post<StoragePolicyActionResult>("/admin/policies/action", data),

	executeSavedPolicyAction: (
		id: number,
		data: ExecuteSavedStoragePolicyActionRequest,
	) =>
		api.post<StoragePolicyActionResult>(`/admin/policies/${id}/action`, data),

	promoteS3CompatibleDriver: (
		id: number,
		data: PromoteS3CompatiblePolicyDriverRequest,
	) => api.post<StoragePolicy>(`/admin/policies/${id}/promote-s3-driver`, data),

	listStorageCredentialProviders: () =>
		api.get<StorageCredentialProviderInfo[]>(
			"/admin/policies/storage-credential-providers",
		),

	startStorageAuthorization: (
		id: number,
		data: StartStorageAuthorizationRequest,
	) =>
		api.post<StorageAuthorizationStartResponse>(
			`/admin/policies/${id}/storage-authorization/start`,
			data,
		),

	listStorageCredentials: (id: number) =>
		api.get<StoragePolicyCredentialInfo[]>(
			`/admin/policies/${id}/storage-credentials`,
		),

	validateStorageCredential: (
		id: number,
		provider: StorageCredentialProvider,
	) =>
		api.post<StoragePolicyCredentialValidationResult>(
			`/admin/policies/${id}/storage-credentials/${provider}/validate`,
		),

	createMigration: (data: CreateStoragePolicyMigrationRequest) =>
		api.post<TaskInfo>("/admin/storage-migrations", data),

	dryRunMigration: (data: DryRunStoragePolicyMigrationRequest) =>
		api.post<StoragePolicyMigrationDryRun>(
			"/admin/storage-migrations/dry-run",
			data,
		),
};

export const adminFolderService = {
	setPolicy: (id: number, data: SetFolderPolicyRequest) =>
		api.put<FolderInfo>(`/admin/folders/${id}/policy`, data),
};

export const adminRemoteNodeService = {
	list: (params?: AdminRemoteNodeListQuery) =>
		api.get<RemoteNodePage>(
			withQuery("/admin/remote-nodes", {
				limit: params?.limit,
				offset: params?.offset,
				sort_by: params?.sort_by,
				sort_order: params?.sort_order,
			}),
		),

	get: (id: number) => api.get<RemoteNodeInfo>(`/admin/remote-nodes/${id}`),

	create: (data: CreateRemoteNodeRequest) =>
		api.post<RemoteNodeInfo>("/admin/remote-nodes", data),

	update: (id: number, data: UpdateRemoteNodeRequest) =>
		api.patch<RemoteNodeInfo>(`/admin/remote-nodes/${id}`, data),

	delete: (id: number) => api.delete<void>(`/admin/remote-nodes/${id}`),

	testConnection: (id: number) =>
		api.post<RemoteNodeInfo>(`/admin/remote-nodes/${id}/test`),

	testParams: (data: TestRemoteNodeParamsReq) =>
		api.post<RemoteStorageCapabilities>("/admin/remote-nodes/test", data),

	createEnrollmentCommand: (id: number) =>
		api.post<RemoteEnrollmentCommandInfo>(
			`/admin/remote-nodes/${id}/enrollment-token`,
		),

	listIngressProfiles: (id: number) =>
		api.get<RemoteIngressProfileInfo[]>(
			`/admin/remote-nodes/${id}/ingress-profiles`,
		),

	listIngressProfileDrivers: (id: number) =>
		api.get<ManagedIngressDriverDescriptor[]>(
			`/admin/remote-nodes/${id}/ingress-profile-drivers`,
		),

	createIngressProfile: (id: number, data: RemoteCreateIngressProfileRequest) =>
		api.post<RemoteIngressProfileInfo>(
			`/admin/remote-nodes/${id}/ingress-profiles`,
			data,
		),

	updateIngressProfile: (
		id: number,
		profileKey: string,
		data: RemoteUpdateIngressProfileRequest,
	) =>
		api.patch<RemoteIngressProfileInfo>(
			`/admin/remote-nodes/${id}/ingress-profiles/${encodeURIComponent(profileKey)}`,
			data,
		),

	deleteIngressProfile: (id: number, profileKey: string) =>
		api.delete<void>(
			`/admin/remote-nodes/${id}/ingress-profiles/${encodeURIComponent(profileKey)}`,
		),
};

// --- External Auth Providers ---

export const adminExternalAuthService = {
	listKinds: () =>
		api.get<AdminExternalAuthProviderKindInfo[]>(
			"/admin/external-auth/provider-kinds",
		),

	list: (params?: AdminExternalAuthProviderListQuery) =>
		api.get<AdminExternalAuthProviderPage>(
			withQuery("/admin/external-auth/providers", {
				limit: params?.limit,
				offset: params?.offset,
			}),
		),

	get: (id: number) =>
		api.get<AdminExternalAuthProviderInfo>(
			`/admin/external-auth/providers/${id}`,
		),

	create: (data: CreateExternalAuthProviderInput) =>
		api.post<AdminExternalAuthProviderInfo>(
			"/admin/external-auth/providers",
			data,
		),

	update: (id: number, data: UpdateExternalAuthProviderInput) =>
		api.patch<AdminExternalAuthProviderInfo>(
			`/admin/external-auth/providers/${id}`,
			data,
		),

	delete: (id: number) =>
		api.delete<void>(`/admin/external-auth/providers/${id}`),

	test: (id: number) =>
		api.post<ExternalAuthProviderTestResult>(
			`/admin/external-auth/providers/${id}/test`,
		),

	testParams: (data: ExternalAuthProviderTestParamsInput) =>
		api.post<ExternalAuthProviderTestResult>(
			"/admin/external-auth/providers/test",
			data,
		),
};

// --- Policy Groups ---

export const adminPolicyGroupService = {
	list: (params?: AdminPolicyGroupListQuery) =>
		api.get<StoragePolicyGroupPage>(
			withQuery("/admin/policy-groups", {
				limit: params?.limit,
				offset: params?.offset,
				sort_by: params?.sort_by,
				sort_order: params?.sort_order,
			}),
		),

	listAll: async (pageSize = 100) => {
		if (!Number.isInteger(pageSize) || pageSize <= 0) {
			throw new Error("pageSize must be a positive integer");
		}

		const allGroups: StoragePolicyGroup[] = [];
		let offset = 0;
		let total = 0;
		let pageCount = 0;
		let maxPages = Number.POSITIVE_INFINITY;

		do {
			pageCount += 1;
			if (pageCount > maxPages) {
				throw new Error("pagination exceeded max iterations");
			}

			const previousOffset = offset;
			const previousCount = allGroups.length;
			const page = await adminPolicyGroupService.list({
				limit: pageSize,
				offset,
			});
			allGroups.push(...page.items);
			total = page.total;
			maxPages = Math.max(1, Math.ceil(total / pageSize)) + 2;
			offset += page.items.length;
			if (page.items.length === 0) {
				if (allGroups.length < total) {
					throw new Error("incomplete pages from adminPolicyGroupService.list");
				}
				break;
			}
			if (offset <= previousOffset || allGroups.length <= previousCount) {
				throw new Error("pagination did not make progress");
			}
		} while (allGroups.length < total);

		return allGroups;
	},

	get: (id: number) =>
		api.get<StoragePolicyGroup>(`/admin/policy-groups/${id}`),

	create: (data: CreatePolicyGroupRequest) =>
		api.post<StoragePolicyGroup>("/admin/policy-groups", data),

	update: (id: number, data: UpdatePolicyGroupRequest) =>
		api.patch<StoragePolicyGroup>(`/admin/policy-groups/${id}`, data),

	delete: (id: number) => api.delete<void>(`/admin/policy-groups/${id}`),

	migrateAssignments: (
		id: number,
		data: MigratePolicyGroupAssignmentsRequest,
	) =>
		api.post<PolicyGroupAssignmentMigrationResult>(
			`/admin/policy-groups/${id}/migrate-assignments`,
			data,
		),
};

// --- WebDAV Locks ---

export type WebdavLock = LockPage["items"][number];
export type AdminShare = ShareInfo;

export const adminShareService = {
	list: (params?: AdminShareListQuery) =>
		api.get<AdminSharePage>(
			withQuery("/admin/shares", {
				limit: params?.limit,
				offset: params?.offset,
				sort_by: params?.sort_by,
				sort_order: params?.sort_order,
			}),
		),

	delete: (id: number) => api.delete<void>(`/admin/shares/${id}`),
};

export const adminFileService = {
	listFiles: (params?: AdminFileListQuery) =>
		api.get<AdminFilePage>(
			withQuery("/admin/files", {
				limit: params?.limit,
				offset: params?.offset,
				name: params?.name,
				blob_id: params?.blob_id,
				policy_id: params?.policy_id,
				owner_user_id: params?.owner_user_id,
				team_id: params?.team_id,
				deleted: params?.deleted,
				sort_by: params?.sort_by,
				sort_order: params?.sort_order,
			}),
		),

	getFile: (id: number) => api.get<AdminFileDetail>(`/admin/files/${id}`),

	listBlobs: (params?: AdminFileBlobListQuery) =>
		api.get<AdminFileBlobPage>(
			withQuery("/admin/file-blobs", {
				limit: params?.limit,
				offset: params?.offset,
				hash: params?.hash,
				policy_id: params?.policy_id,
				storage_path: params?.storage_path,
				ref_count_min: params?.ref_count_min,
				ref_count_max: params?.ref_count_max,
				size_min: params?.size_min,
				size_max: params?.size_max,
				sort_by: params?.sort_by,
				sort_order: params?.sort_order,
			}),
		),

	getBlob: (id: number) =>
		api.get<AdminFileBlobDetail>(`/admin/file-blobs/${id}`),

	createBlobMaintenanceTask: (body: CreateBlobMaintenanceTaskRequest) =>
		api.post<TaskInfo>("/admin/file-blobs/maintenance", body),
};

export const adminTaskService = {
	list: (params?: AdminTaskListQuery) =>
		api.get<TaskPage>(
			withQuery("/admin/tasks", {
				limit: params?.limit,
				offset: params?.offset,
				kind: params?.kind,
				status: params?.status,
				sort_by: params?.sort_by,
				sort_order: params?.sort_order,
			}),
		),

	cleanupCompleted: (data: AdminTaskCleanupRequest) =>
		api.post<RemovedCountResponse>("/admin/tasks/cleanup", data),

	resumeStoragePolicyMigration: (taskId: number) =>
		api.post<TaskInfo>(`/admin/storage-migrations/${taskId}/resume`),
};

export const adminLockService = {
	list: (params?: AdminLockListQuery) =>
		api.get<LockPage>(
			withQuery("/admin/locks", {
				limit: params?.limit,
				offset: params?.offset,
				sort_by: params?.sort_by,
				sort_order: params?.sort_order,
			}),
		),

	forceUnlock: (id: number) => api.delete<void>(`/admin/locks/${id}`),

	cleanupExpired: () =>
		api.delete<RemovedCountResponse>("/admin/locks/expired"),
};

export const adminConfigService = {
	list: (params?: AdminConfigListQuery) =>
		api.get<SystemConfigPage>(
			withQuery("/admin/config", {
				limit: params?.limit,
				offset: params?.offset,
			}),
		),

	schema: () => api.get<ConfigSchemaItem[]>("/admin/config/schema"),

	templateVariables: () =>
		api.get<TemplateVariableGroup[]>("/admin/config/template-variables"),

	get: (key: string) => api.get<SystemConfig>(`/admin/config/${key}`),

	set: (
		key: string,
		value: string | string[],
		visibility?: SystemConfigVisibility,
	) =>
		api.put<SystemConfig>(
			`/admin/config/${key}`,
			visibility ? { value, visibility } : { value },
		),

	delete: (key: string) => api.delete<void>(`/admin/config/${key}`),

	action: (key: string, data: ExecuteConfigActionRequest) =>
		api.post<ExecuteConfigActionResponse>(`/admin/config/${key}/action`, data),

	sendTestEmail: (targetEmail?: string) =>
		api.post<ActionMessageResp>("/admin/config/mail/action", {
			action: "send_test_email" satisfies ConfigActionType,
			target_email: targetEmail,
		}),
};
