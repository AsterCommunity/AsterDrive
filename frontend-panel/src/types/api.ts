// Re-export generated types for convenience
import type {
	operations as ApiOperations,
	components,
} from "@/services/api.generated";

export type { operations, paths } from "@/services/api.generated";

type OperationQuery<Operation extends keyof ApiOperations> =
	ApiOperations[Operation] extends { parameters: { query?: infer Query } }
		? NonNullable<Query>
		: never;

type OperationData<Operation extends keyof ApiOperations> =
	ApiOperations[Operation] extends {
		responses: {
			200: {
				content: {
					"application/json": {
						data?: infer Data;
					};
				};
			};
		};
	}
		? NonNullable<Data>
		: never;

// Core responses
export type ErrorCode = components["schemas"]["ErrorCode"];
export type ApiErrorCode = components["schemas"]["ApiErrorCode"];
export type ApiErrorInfo = components["schemas"]["ApiErrorInfo"];
export type ApiSubcode = components["schemas"]["ApiSubcode"];
// TODO(0.3.0): revisit this wrapper when the backend response envelope migrates
// from numeric ErrorCode/subcode compatibility to ApiErrorCode-only responses.
// OpenAPI expands Rust ApiResponse<T> into concrete ApiResponse_X schemas, so
// the frontend keeps this thin generic alias as the single local entry point.
export type ApiResponse<T = unknown> = {
	code: ErrorCode;
	msg: string;
	data?: T | null;
	error?: ApiErrorInfo | null;
};
export type HealthResponse = components["schemas"]["HealthResponse"];
export type MemoryStatsResponse = components["schemas"]["MemoryStatsResponse"];

// Auth and user
export type AvatarInfo = components["schemas"]["AvatarInfo"];
export type AvatarSource = components["schemas"]["AvatarSource"];
export type ActionMessageResp = components["schemas"]["ActionMessageResp"];
export type AuthSessionInfo = components["schemas"]["AuthSessionInfo"];
export type AuthTokenResp = components["schemas"]["AuthTokenResp"];
export type CheckResp = components["schemas"]["CheckResp"];
export type ChangePasswordRequest = components["schemas"]["ChangePasswordReq"];
export type CreateUserReq = components["schemas"]["CreateUserReq"];
export type MeResponse = components["schemas"]["MeResponse"];
export type MePartialResponse = components["schemas"]["MePartialResponse"];
export type MeField = "profile" | "preferences" | "quota" | "session";
export type PasswordResetConfirmRequest =
	components["schemas"]["PasswordResetConfirmReq"];
export type PasswordResetRequestRequest =
	components["schemas"]["PasswordResetRequestReq"];
export type ExternalAuthLinkInfo =
	components["schemas"]["ExternalAuthLinkInfo"];
export type ExternalAuthEmailVerificationStartRequest =
	components["schemas"]["ExternalAuthEmailVerificationStartRequest"];
export type ExternalAuthEmailVerificationStartResponse =
	OperationData<"start_external_auth_email_verification">;
export type ExternalAuthPasswordLinkRequest =
	components["schemas"]["ExternalAuthPasswordLinkRequest"];
export type ExternalAuthPublicProvider =
	components["schemas"]["ExternalAuthPublicProvider"];
export type ExternalAuthStartLoginRequest =
	components["schemas"]["ExternalAuthStartLoginRequest"];
export type ExternalAuthStartLoginResponse =
	components["schemas"]["ExternalAuthStartLoginResponse"];
export type PasskeyInfo = OperationData<"list_passkeys">[number];
export type PasskeyLoginFinishRequest =
	components["schemas"]["PasskeyLoginFinishReq"];
export type PasskeyLoginStartRequest =
	components["schemas"]["PasskeyLoginStartReq"];
export type PasskeyLoginStartResponse = OperationData<"start_passkey_login">;
export type PasskeyRegisterFinishRequest =
	components["schemas"]["PasskeyRegisterFinishReq"];
export type PasskeyRegisterStartRequest =
	components["schemas"]["PasskeyRegisterStartReq"];
export type PasskeyRegisterStartResponse =
	OperationData<"start_passkey_registration">;
export type PatchPasskeyRequest = components["schemas"]["PatchPasskeyReq"];
export type RequestEmailChangeRequest =
	components["schemas"]["RequestEmailChangeReq"];
export type ResendRegisterActivationRequest =
	components["schemas"]["ResendRegisterActivationReq"];
export type ContactVerificationConfirmQuery =
	components["schemas"]["ContactVerificationConfirmQuery"];
export type MeQuery = OperationQuery<"me">;
export type UpdateAvatarSourceRequest =
	components["schemas"]["UpdateAvatarSourceReq"];
export type UpdatePreferencesRequest =
	components["schemas"]["UpdatePreferencesReq"];
export type UpdateProfileRequest = components["schemas"]["UpdateProfileReq"];
export type UserInfo = components["schemas"]["UserInfo"];
export type UserPage = components["schemas"]["OffsetPage_UserInfo"];
export type UserPreferences = components["schemas"]["UserPreferences"];
export type UserProfileInfo = components["schemas"]["UserProfileInfo"];
export type UserRole = components["schemas"]["UserRole"];
export type UserSummary = components["schemas"]["UserSummary"];
export type UserStatus = components["schemas"]["UserStatus"];
export type VerificationPurpose = components["schemas"]["VerificationPurpose"];

// Files, folders, and trash
export type FileCategory = components["schemas"]["FileCategory"];
export type FileInfo = components["schemas"]["FileInfo"];
export type FileListItem = components["schemas"]["FileListItem"];
export type FileVersion = components["schemas"]["FileVersion"];
export type FolderAncestorItem = components["schemas"]["FolderAncestorItem"];
export type FolderContents = components["schemas"]["FolderContents"];
export type FolderInfo = components["schemas"]["FolderInfo"];
export type FolderListParams = OperationQuery<"list_root">;
export type FolderListItem = components["schemas"]["FolderListItem"];
export type ArchivePreviewEntry = components["schemas"]["ArchivePreviewEntry"];
export type ArchivePreviewEntryKind =
	components["schemas"]["ArchivePreviewEntryKind"];
export type ArchivePreviewManifest =
	components["schemas"]["ArchivePreviewManifest"];
export type ArchiveFilenameEncoding =
	components["schemas"]["ArchiveFilenameEncoding"];
export type PurgedCountResponse = components["schemas"]["PurgedCountResponse"];
export type TrashContents = components["schemas"]["TrashContents"];
export type TrashFileItem = components["schemas"]["TrashFileItem"];
export type TrashFolderItem = components["schemas"]["TrashFolderItem"];
export type TrashListParams = OperationQuery<"list_trash">;
export type MediaMetadataKind = components["schemas"]["MediaMetadataKind"];
export type MediaMetadataStatus = components["schemas"]["MediaMetadataStatus"];
export type ImageMediaMetadata = components["schemas"]["ImageMediaMetadata"];
export type AudioMediaMetadata = components["schemas"]["AudioMediaMetadata"];
export type VideoMediaMetadata = components["schemas"]["VideoMediaMetadata"];
export type MediaMetadataPayload =
	components["schemas"]["MediaMetadataPayload"];
export type MediaMetadataInfo = components["schemas"]["MediaMetadataInfo"];

// Sharing and search
export type AdminSharePage = components["schemas"]["OffsetPage_ShareInfo"];
export type DirectLinkTokenInfo = components["schemas"]["DirectLinkTokenInfo"];
export type DirectLinkQuery = components["schemas"]["DirectLinkQuery"];
export type FileSearchItem = components["schemas"]["FileSearchItem"];
export type MyShareInfo = components["schemas"]["MyShareInfo"];
export type PreviewLinkInfo = components["schemas"]["PreviewLinkInfo"];
export type ShareStreamSessionInfo =
	components["schemas"]["ShareStreamSessionInfo"];
export type SearchParams = components["schemas"]["SearchParams"];
export type SearchResults = components["schemas"]["SearchResults"];
export type ShareInfo = components["schemas"]["ShareInfo"];
export type SharePage = components["schemas"]["OffsetPage_MyShareInfo"];
export type ShareListQuery = OperationQuery<"list_my_shares">;
export type SharePublicInfo = components["schemas"]["SharePublicInfo"];
export type ShareStatus = components["schemas"]["ShareStatus"];
export type ShareTarget = components["schemas"]["ShareTarget"];
export type StorageChangeEvent = components["schemas"]["StorageChangeEvent"];
export type StorageChangeKind = components["schemas"]["StorageChangeKind"];
export type StorageChangeWorkspace =
	components["schemas"]["StorageChangeWorkspace"];
export type TeamShareListQuery = OperationQuery<"list_team_shares">;

// Admin, storage, and WebDAV
export type AuditAction = components["schemas"]["AuditAction"];
export type AuditEntityType = components["schemas"]["AuditEntityType"];
export type AuditLogEntry = components["schemas"]["AuditLogEntry"];
export type AuditLogPage = components["schemas"]["OffsetPage_AuditLogEntry"];
export type AdminAuditLogSortBy = components["schemas"]["AdminAuditLogSortBy"];
export type AdminLockSortBy = components["schemas"]["AdminLockSortBy"];
export type AdminOverview = components["schemas"]["AdminOverview"];
export type AuditLogListQuery = OperationQuery<"list_audit_logs">;
export type AdminOverviewQuery = OperationQuery<"get_admin_overview">;
export type AdminOverviewDailyReport =
	components["schemas"]["AdminOverviewDailyReport"];
export type AdminOverviewStats = components["schemas"]["AdminOverviewStats"];
export type AdminSystemHealthStatus =
	components["schemas"]["AdminSystemHealthStatus"];
export type AdminSystemHealthSummary =
	components["schemas"]["AdminSystemHealthSummary"];
export type ExternalAuthProviderKind =
	components["schemas"]["ExternalAuthProviderKind"];
export type ExternalAuthProtocol =
	components["schemas"]["ExternalAuthProtocol"];
export type AdminExternalAuthProviderKindInfo =
	OperationData<"admin_list_external_auth_provider_kinds">[number];
export type AdminExternalAuthProviderInfo =
	components["schemas"]["AdminExternalAuthProviderInfo"];
export type AdminExternalAuthProviderPage =
	components["schemas"]["OffsetPage_AdminExternalAuthProviderInfo"];
export type AdminExternalAuthProviderListQuery =
	OperationQuery<"admin_list_external_auth_providers">;
export type CreateExternalAuthProviderInput =
	components["schemas"]["CreateExternalAuthProviderInput"];
export type UpdateExternalAuthProviderInput =
	components["schemas"]["UpdateExternalAuthProviderInput"];
export type ExternalAuthProviderTestParamsInput =
	components["schemas"]["ExternalAuthProviderTestParamsInput"];
export type ExternalAuthProviderTestResult =
	components["schemas"]["ExternalAuthProviderTestResult"];
export type AdminCreateTeamRequest =
	components["schemas"]["AdminCreateTeamReq"];
export type AdminTeamInfo = components["schemas"]["AdminTeamInfo"];
export type AdminTeamPage = components["schemas"]["OffsetPage_AdminTeamInfo"];
export type AdminTeamSortBy = components["schemas"]["AdminTeamSortBy"];
export type AdminUpdateTeamRequest = components["schemas"]["AdminPatchTeamReq"];
export type AdminTeamListQuery = OperationQuery<"admin_list_teams">;
export type AdminTeamAuditLogListQuery =
	OperationQuery<"admin_list_team_audit_logs">;
export type AdminTeamMemberListQuery =
	OperationQuery<"admin_list_team_members">;
export type AdminTeamMemberSortBy =
	components["schemas"]["AdminTeamMemberSortBy"];
export type TeamAuditEntryInfo = components["schemas"]["TeamAuditEntryInfo"];
export type TeamAuditPage =
	components["schemas"]["OffsetPage_TeamAuditEntryInfo"];
export type TeamMemberPage = components["schemas"]["TeamMemberPage"];
export type BackgroundTaskKind =
	| components["schemas"]["BackgroundTaskKind"]
	| "archive_preview_generate";
export type BackgroundTaskStatus =
	components["schemas"]["BackgroundTaskStatus"];
export type AdminUserListQuery = OperationQuery<"list_users">;
export type AdminUserSortBy = components["schemas"]["AdminUserSortBy"];
export type CreatePolicyGroupRequest =
	components["schemas"]["CreatePolicyGroupReq"];
export type CreatePolicyRequest = components["schemas"]["CreatePolicyReq"];
export type AdminPolicyListQuery = OperationQuery<"list_policies">;
export type AdminPolicySortBy = components["schemas"]["AdminPolicySortBy"];
export type AdminPolicyGroupSortBy =
	components["schemas"]["AdminPolicyGroupSortBy"];
export type DeletePolicyQuery = OperationQuery<"delete_policy">;
export type CreateRemoteNodeRequest =
	components["schemas"]["CreateRemoteNodeReq"];
export type AdminRemoteNodeListQuery = OperationQuery<"list_remote_nodes">;
export type AdminRemoteNodeSortBy =
	components["schemas"]["AdminRemoteNodeSortBy"];
export type AdminShareSortBy = components["schemas"]["AdminShareSortBy"];
export type AdminTaskSortBy = components["schemas"]["AdminTaskSortBy"];
export type DriverType = components["schemas"]["DriverType"];
export type MediaProcessorKind = components["schemas"]["MediaProcessorKind"];
export type LockPage = components["schemas"]["OffsetPage_ResourceLock"];
export type RemoteCreateIngressProfileRequest =
	components["schemas"]["RemoteCreateIngressProfileRequest"];
export type RemoteEnrollmentCommandInfo =
	components["schemas"]["RemoteEnrollmentCommandInfo"];
export type RemoteIngressProfileInfo =
	components["schemas"]["RemoteIngressProfileInfo"];
export type RemoteNodeEnrollmentStatus =
	components["schemas"]["RemoteNodeEnrollmentStatus"];
export type RemoteNodeInfo = components["schemas"]["RemoteNodeInfo"];
export type RemoteNodePage = components["schemas"]["OffsetPage_RemoteNodeInfo"];
export type RemoteStorageCapabilities =
	components["schemas"]["RemoteStorageCapabilities"];
export type RemoteUpdateIngressProfileRequest =
	components["schemas"]["RemoteUpdateIngressProfileRequest"];
export type RemoteDownloadStrategy =
	components["schemas"]["RemoteDownloadStrategy"];
export type RemoteUploadStrategy =
	components["schemas"]["RemoteUploadStrategy"];
export type ResourceLockOwnerInfo =
	components["schemas"]["ResourceLockOwnerInfo"];
export type S3UploadStrategy = components["schemas"]["S3UploadStrategy"];
export type StoragePolicyOptions =
	components["schemas"]["StoragePolicyOptions"];
export type MigratePolicyGroupUsersRequest =
	components["schemas"]["MigratePolicyGroupUsersReq"];
export type PolicyGroupItemRequest =
	components["schemas"]["PolicyGroupItemReq"];
export type AdminPolicyGroupListQuery = OperationQuery<"list_policy_groups">;
export type UpdatePolicyGroupRequest =
	components["schemas"]["PatchPolicyGroupReq"];
export type UpdatePolicyRequest = components["schemas"]["PatchPolicyReq"];
export type PatchRemoteNodeReq = components["schemas"]["PatchRemoteNodeReq"];
export type TestPolicyParamsRequest =
	components["schemas"]["TestPolicyParamsReq"];
export type TestRemoteNodeParamsReq =
	components["schemas"]["TestRemoteNodeParamsReq"];
export type UpdateRemoteNodeRequest =
	components["schemas"]["PatchRemoteNodeReq"];
export type UpdateUserRequest = components["schemas"]["PatchUserReq"];
export type RemovedCountResponse =
	components["schemas"]["RemovedCountResponse"];
export type ResetUserPasswordRequest =
	components["schemas"]["ResetUserPasswordReq"];
export type StoragePolicy = components["schemas"]["StoragePolicy"];
export type StoragePolicyPage =
	components["schemas"]["OffsetPage_StoragePolicy"];
export type S3DownloadStrategy = components["schemas"]["S3DownloadStrategy"];
export type StoragePolicyGroupItem =
	components["schemas"]["StoragePolicyGroupItemInfo"];
export type StoragePolicyGroup =
	components["schemas"]["StoragePolicyGroupInfo"];
export type PolicyGroupUserMigrationResult =
	components["schemas"]["PolicyGroupUserMigrationResult"];
export type StoragePolicyGroupPage =
	components["schemas"]["OffsetPage_StoragePolicyGroupInfo"];
export type AdminShareListQuery = OperationQuery<"list_all_shares">;
export type AdminTaskListQuery = Omit<
	OperationQuery<"admin_list_tasks">,
	"kind"
> & {
	kind?: BackgroundTaskKind | null;
};
export type AdminTaskCleanupRequest = Omit<
	components["schemas"]["AdminTaskCleanupReq"],
	"kind"
> & {
	kind?: BackgroundTaskKind | null;
};
export type AdminLockListQuery = OperationQuery<"list_locks">;
export type ConfigActionType = components["schemas"]["ConfigActionType"];
export type ConfigSchemaItem = components["schemas"]["ConfigSchemaItem"];
export type AdminConfigListQuery = OperationQuery<"list_config">;
export type ExecuteConfigActionRequest =
	components["schemas"]["ExecuteConfigActionReq"];
export type ExecuteConfigActionResponse =
	components["schemas"]["ExecuteConfigActionResp"];
export type TemplateVariableGroup =
	components["schemas"]["TemplateVariableGroup"];
export type TemplateVariableItem =
	components["schemas"]["TemplateVariableItem"];
export type PublicBranding = components["schemas"]["PublicBranding"];
export type PreviewAppProvider = components["schemas"]["PreviewAppProvider"];
export type PreviewOpenMode = components["schemas"]["PreviewOpenMode"];
export type PublicPreviewAppConfig =
	components["schemas"]["PublicPreviewAppConfig"];
export type PublicPreviewAppDefinition =
	components["schemas"]["PublicPreviewAppDefinition"];
export type PublicPreviewAppsConfig =
	components["schemas"]["PublicPreviewAppsConfig"];
export type PublicThumbnailSupport =
	components["schemas"]["PublicThumbnailSupport"];
export type PublicMediaDataKindSupport =
	components["schemas"]["PublicMediaDataKindSupport"];
export type PublicMediaDataKindsSupport =
	components["schemas"]["PublicMediaDataKindsSupport"];
export type PublicMediaDataSupport =
	components["schemas"]["PublicMediaDataSupport"];
export type PublicMediaDataSupportMatch =
	components["schemas"]["PublicMediaDataSupportMatch"];
export type SystemConfig = components["schemas"]["SystemConfig"];
export type SystemConfigPage = components["schemas"]["OffsetPage_SystemConfig"];
export type SystemConfigSource = components["schemas"]["SystemConfigSource"];
export type SystemConfigValueType =
	components["schemas"]["SystemConfigValueType"];
export type WebdavAccount = components["schemas"]["WebdavAccount"];
export type WebdavAccountCreated =
	components["schemas"]["WebdavAccountCreated"];
export type WebdavAccountInfo = components["schemas"]["WebdavAccountInfo"];
export type WebdavAccountPage =
	components["schemas"]["OffsetPage_WebdavAccountInfo"];
export type WebdavAccountListQuery = OperationQuery<"list_webdav_accounts">;
export type WebdavSettingsInfo = components["schemas"]["WebdavSettingsInfo"];
export type OpenWopiRequest = components["schemas"]["OpenWopiRequest"];
export type WopiAccessQuery = components["schemas"]["WopiAccessQuery"];
export type WopiLaunchSession = components["schemas"]["WopiLaunchSession"];

// Teams
export type AddTeamMemberRequest = components["schemas"]["AddTeamMemberReq"];
export type CreateTeamRequest = components["schemas"]["CreateTeamReq"];
export type TeamInfo = components["schemas"]["TeamInfo"];
export type TeamListQuery = OperationQuery<"list_teams">;
export type TeamAuditLogListQuery = OperationQuery<"list_team_audit_logs">;
export type TeamMemberInfo = components["schemas"]["TeamMemberInfo"];
export type TeamMemberListQuery = OperationQuery<"list_team_members">;
export type TeamMemberRole = components["schemas"]["TeamMemberRole"];
export type UpdateTeamMemberRequest =
	components["schemas"]["PatchTeamMemberReq"];
export type UpdateTeamRequest = components["schemas"]["PatchTeamReq"];
export type TaskListQuery = OperationQuery<"list_tasks">;
export type TeamTaskListQuery = OperationQuery<"list_team_tasks">;
export type TaskStepStatus = components["schemas"]["TaskStepStatus"];
export type TaskStepInfo = components["schemas"]["TaskStepInfo"];
export type TaskPayload = components["schemas"]["TaskPayload"];
export type TaskResult = components["schemas"]["TaskResult"];
export type TaskInfo = components["schemas"]["TaskInfo"];
export type TaskPage = components["schemas"]["OffsetPage_TaskInfo"];

// Upload and batch
export type BatchItemError = components["schemas"]["BatchItemError"];
export type BatchResult = components["schemas"]["BatchResult"];
export type ChunkUploadResponse = components["schemas"]["ChunkUploadResponse"];
export type CompletedPart = components["schemas"]["CompletedPartReq"];
export type FileQuery = components["schemas"]["FileQuery"];
export type InitUploadResponse = components["schemas"]["InitUploadResponse"];
export type RecoverableUploadPart =
	components["schemas"]["RecoverableUploadPartResponse"];
export type RecoverableUploadSession = NonNullable<
	NonNullable<
		ApiOperations["list_recoverable_upload_sessions"]["responses"][200]["content"]
	>["application/json"]["data"]
>[number];
export type UploadMode = components["schemas"]["UploadMode"];
export type UploadProgressResponse =
	components["schemas"]["UploadProgressResponse"];
export type UploadSessionStatus = components["schemas"]["UploadSessionStatus"];
