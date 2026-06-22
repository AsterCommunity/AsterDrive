// Re-export generated types for convenience
// IMPORTANT: Agent code should import from this file instead of api.generated.ts to avoid coupling to the codegen tool and to allow manual additions of types as needed.
// It is strictly prohibited to directly add new fields in this document.
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
		: ApiOperations[Operation] extends {
					responses: {
						201: {
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
export type ApiErrorCode = components["schemas"]["ApiErrorCode"];
export type ApiErrorInfo = components["schemas"]["ApiErrorInfo"];
// OpenAPI expands Rust ApiResponse<T> into concrete ApiResponse_X schemas, so
// the frontend keeps this thin generic alias as the single local entry point.
export type ApiResponse<T = unknown> = {
	code: ApiErrorCode;
	msg: string;
	data?: T | null;
	error?: ApiErrorInfo | null;
};
export type HealthResponse = components["schemas"]["HealthResponse"];
export type MemoryStatsResponse = components["schemas"]["MemoryStatsResponse"];
export type SystemInfoResponse = components["schemas"]["SystemInfoResponse"];

// Auth and user
export type AvatarInfo = components["schemas"]["AvatarInfo"];
export type AvatarSource = components["schemas"]["AvatarSource"];
export type ActionMessageResp = components["schemas"]["ActionMessageResp"];
export type AuthSessionInfo = components["schemas"]["AuthSessionInfo"];
export type AuthTokenResp = components["schemas"]["AuthTokenResp"];
export type CheckResp = components["schemas"]["CheckResp"];
export type ChangePasswordRequest = components["schemas"]["ChangePasswordReq"];
export type CreateUserReq = components["schemas"]["CreateUserReq"];
export type CreateUserResponse = OperationData<"create_user">;
export type CreateUserInvitationRequest =
	components["schemas"]["CreateUserInvitationReq"];
export type MeResponse = components["schemas"]["MeResponse"];
export type MePartialResponse = components["schemas"]["MePartialResponse"];
export type MeField = "profile" | "preferences" | "quota" | "session";
export type PasswordResetConfirmRequest =
	components["schemas"]["PasswordResetConfirmReq"];
export type PasswordResetRequestRequest =
	components["schemas"]["PasswordResetRequestReq"];
export type AcceptUserInvitationRequest =
	components["schemas"]["AcceptUserInvitationReq"];
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
export type PublicUserInvitationInfo =
	components["schemas"]["PublicUserInvitationInfo"];
export type UserInvitationStatus =
	components["schemas"]["UserInvitationStatus"];
export type VerificationPurpose = components["schemas"]["VerificationPurpose"];

// Files, folders, and trash
export type FileCategory = components["schemas"]["FileCategory"];
export type FileInfo = components["schemas"]["FileInfo"];
export type FileListItem = components["schemas"]["FileListItem"];
export type FileResourceDeliveryMode =
	components["schemas"]["FileResourceDeliveryMode"];
export type FileResourceHandleRequest =
	components["schemas"]["FileResourceHandleRequest"];
export type FileResourcePurpose = components["schemas"]["FileResourcePurpose"];
export type FileResourceRepresentation =
	components["schemas"]["FileResourceRepresentation"];
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
export type BatchTagBindingRequest =
	components["schemas"]["BatchTagBindingReq"];
export type CreateTagRequest = components["schemas"]["CreateTagReq"];
export type EntityTags = components["schemas"]["EntityTags"];
export type EntityType = components["schemas"]["EntityType"];
export type PatchTagRequest = components["schemas"]["PatchTagReq"];
export type ReplaceEntityTagsRequest =
	components["schemas"]["ReplaceEntityTagsReq"];
export type TagInfo = components["schemas"]["TagInfo"];
export type TagListParams = OperationQuery<"list_tags">;
export type TagPage = components["schemas"]["OffsetPage_TagInfo"];
export type TagScopeType = components["schemas"]["TagScopeType"];
export type TagSummary = components["schemas"]["TagSummary"];

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
export type BatchDeleteSharesRequest =
	components["schemas"]["BatchDeleteSharesReq"];
export type CreateShareRequest = components["schemas"]["CreateShareReq"];
export type UpdateShareRequest = components["schemas"]["UpdateShareReq"];
export type VerifySharePasswordRequest =
	components["schemas"]["VerifyPasswordReq"];
export type StorageChangeEvent = components["schemas"]["StorageChangeEvent"];
export type StorageChangeKind = components["schemas"]["StorageChangeKind"];
export type StorageChangeWorkspace =
	components["schemas"]["StorageChangeWorkspace"];
export type TeamShareListQuery = OperationQuery<"list_team_shares">;

// Admin, storage, and WebDAV
export type AuditAction = components["schemas"]["AuditAction"];
export type AuditEntityType = components["schemas"]["AuditEntityType"];
export type AuditPresentation = components["schemas"]["AuditPresentation"];
export type AuditPresentationMessage =
	components["schemas"]["AuditPresentationMessage"];
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
export type BackgroundTaskKind = components["schemas"]["BackgroundTaskKind"];
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
export type AdminUserInvitationInfo =
	components["schemas"]["AdminUserInvitationInfo"];
export type AdminUserInvitationPage =
	components["schemas"]["OffsetPage_AdminUserInvitationInfo"];
export type AdminUserInvitationListQuery =
	OperationQuery<"admin_list_user_invitations">;
export type AdminFileSortBy = components["schemas"]["AdminFileSortBy"];
export type AdminFileBlobSortBy = components["schemas"]["AdminFileBlobSortBy"];
export type AdminFileBlobHashKind =
	components["schemas"]["AdminFileBlobHashKind"];
export type AdminFileBlobHealth = components["schemas"]["AdminFileBlobHealth"];
export type AdminFileBlobSummary =
	components["schemas"]["AdminFileBlobSummary"];
export type AdminFileInfo = components["schemas"]["AdminFileInfo"];
export type AdminFileVersionSummary =
	components["schemas"]["AdminFileVersionSummary"];
export type AdminFileDetail = components["schemas"]["AdminFileDetail"];
export type AdminFileBlobInfo = components["schemas"]["AdminFileBlobInfo"];
export type AdminFileBlobReferenceFile =
	components["schemas"]["AdminFileBlobReferenceFile"];
export type AdminFileBlobReferenceVersion =
	components["schemas"]["AdminFileBlobReferenceVersion"];
export type AdminFileBlobDetail = components["schemas"]["AdminFileBlobDetail"];
export type BlobMaintenanceAction =
	components["schemas"]["BlobMaintenanceAction"];
export type CreateBlobMaintenanceTaskRequest =
	components["schemas"]["BlobMaintenanceTaskPayload"];
export type AdminFileListQuery = OperationQuery<"admin_list_files">;
export type AdminFileBlobListQuery = OperationQuery<"admin_list_file_blobs">;
export type AdminFilePage = components["schemas"]["OffsetPage_AdminFileInfo"];
export type AdminFileBlobPage =
	components["schemas"]["OffsetPage_AdminFileBlobInfo"];
export type DriverType = components["schemas"]["DriverType"];
export type MicrosoftGraphCloud = components["schemas"]["MicrosoftGraphCloud"];
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
export type RemoteNodeTransportMode =
	components["schemas"]["RemoteNodeTransportMode"];
export type RemoteStorageCapabilities =
	components["schemas"]["RemoteStorageCapabilities"];
export type RemoteTunnelInfo = components["schemas"]["RemoteTunnelInfo"];
export type RemoteUpdateIngressProfileRequest =
	components["schemas"]["RemoteUpdateIngressProfileRequest"];
export type RemoteDownloadStrategy =
	components["schemas"]["RemoteDownloadStrategy"];
export type RemoteUploadStrategy =
	components["schemas"]["RemoteUploadStrategy"];
export type ResourceLockOwnerInfo =
	components["schemas"]["ResourceLockOwnerInfo"];
export type ObjectStorageUploadStrategy =
	components["schemas"]["ObjectStorageUploadStrategy"];
export type StoragePolicyOptions =
	components["schemas"]["StoragePolicyOptions"];
export type OneDriveAccountMode = components["schemas"]["OneDriveAccountMode"];
export type StartStorageAuthorizationRequest =
	components["schemas"]["StartStorageAuthorizationReq"];
export type StorageAuthorizationStartResponse =
	components["schemas"]["StorageAuthorizationStartResponse"];
export type StorageCredentialProvider =
	components["schemas"]["StorageCredentialProvider"];
export type StorageCredentialProviderInfo =
	components["schemas"]["StorageCredentialProviderInfo"];
export type StoragePolicyCredentialInfo =
	components["schemas"]["StoragePolicyCredentialInfo"];
export type StoragePolicyCredentialValidationResult =
	components["schemas"]["StoragePolicyCredentialValidationResult"];
export type MigratePolicyGroupAssignmentsRequest =
	components["schemas"]["MigratePolicyGroupAssignmentsReq"];
export type PolicyGroupItemRequest =
	components["schemas"]["PolicyGroupItemReq"];
export type AdminPolicyGroupListQuery = OperationQuery<"list_policy_groups">;
export type UpdatePolicyGroupRequest =
	components["schemas"]["PatchPolicyGroupReq"];
export type UpdatePolicyRequest = components["schemas"]["PatchPolicyReq"];
export type PatchRemoteNodeReq = components["schemas"]["PatchRemoteNodeReq"];
export type TestPolicyParamsRequest =
	components["schemas"]["TestPolicyParamsReq"];
export type ExecuteDraftStoragePolicyActionRequest =
	components["schemas"]["ExecuteDraftStoragePolicyActionReq"];
export type ExecuteSavedStoragePolicyActionRequest =
	components["schemas"]["ExecuteSavedStoragePolicyActionReq"];
export type StoragePolicyActionResult =
	OperationData<"execute_draft_storage_policy_action">;
export type StoragePolicyConnectionTestResult =
	OperationData<"test_policy_params">;
export type StoragePolicyExecutableAction =
	components["schemas"]["StoragePolicyExecutableAction"];
export type PromoteS3CompatiblePolicyDriverRequest =
	components["schemas"]["PromoteS3CompatiblePolicyDriverReq"];
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
export type StoragePolicyCapacityInfo = OperationData<"get_policy_capacity">;
export type StorageConnectorDescriptor =
	OperationData<"list_storage_driver_descriptors">[number];
export type StorageConnectorFieldDescriptor =
	components["schemas"]["StorageConnectorFieldDescriptor"];
export type StorageConnectorUiDescriptor =
	components["schemas"]["StorageConnectorUiDescriptor"];
export type StorageConnectorAffordanceAction =
	components["schemas"]["StorageConnectorAffordanceAction"];
export type StorageConnectorActionKind =
	components["schemas"]["StorageConnectorActionKind"];
export type StoragePolicyPage =
	components["schemas"]["OffsetPage_StoragePolicy"];
export type CreateStoragePolicyMigrationRequest =
	components["schemas"]["CreateStoragePolicyMigrationReq"];
export type DryRunStoragePolicyMigrationRequest =
	components["schemas"]["DryRunStoragePolicyMigrationReq"];
export type StoragePolicyMigrationDryRun =
	OperationData<"dry_run_storage_policy_migration">;
export type StoragePolicyMigrationTaskPayload =
	components["schemas"]["StoragePolicyMigrationTaskPayload"];
export type StoragePolicyMigrationTaskResult =
	components["schemas"]["StoragePolicyMigrationTaskResult"];
export type CreateOfflineDownloadTaskParams =
	components["schemas"]["CreateOfflineDownloadTaskParams"];
export type OfflineDownloadTaskPayloadInfo =
	components["schemas"]["OfflineDownloadTaskPayloadInfo"];
export type OfflineDownloadTaskResult =
	components["schemas"]["OfflineDownloadTaskResult"];
export type ObjectStorageDownloadStrategy =
	components["schemas"]["ObjectStorageDownloadStrategy"];
export type StoragePolicySummaryInfo =
	components["schemas"]["StoragePolicySummaryInfo"];
export type StoragePolicyGroupItem =
	components["schemas"]["StoragePolicyGroupItemInfo"];
export type StoragePolicyGroup =
	components["schemas"]["StoragePolicyGroupInfo"];
export type PolicyGroupAssignmentMigrationResult =
	components["schemas"]["PolicyGroupAssignmentMigrationResult"];
export type StoragePolicyGroupPage =
	components["schemas"]["OffsetPage_StoragePolicyGroupInfo"];
export type AdminShareListQuery = OperationQuery<"list_all_shares">;
export type AdminTaskListQuery = OperationQuery<"admin_list_tasks">;
export type AdminTaskCleanupRequest =
	components["schemas"]["AdminTaskCleanupReq"];
export type AdminLockListQuery = OperationQuery<"list_locks">;
export type ConfigActionType = components["schemas"]["ConfigActionType"];
export type ConfigSchemaItem = components["schemas"]["ConfigSchemaItem"];
export type ConfigSchemaOption = components["schemas"]["ConfigSchemaOption"];
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
export type PublicExtensionSupport =
	components["schemas"]["PublicExtensionSupport"];
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
export type PublicImagePreviewPreference =
	components["schemas"]["PublicImagePreviewPreference"];
export type PublicFrontendMediaConfig =
	components["schemas"]["PublicFrontendMediaConfig"];
export type PublicFrontendConfig =
	components["schemas"]["PublicFrontendConfig"];
export type SystemConfig = components["schemas"]["SystemConfig"];
export type SystemConfigPage = components["schemas"]["OffsetPage_SystemConfig"];
export type SystemConfigSource = components["schemas"]["SystemConfigSource"];
export type SystemConfigVisibility = "private" | "public" | "authenticated";
export type SystemConfigValueType =
	components["schemas"]["SystemConfigValueType"];
export type WebdavAccount = components["schemas"]["WebdavAccount"];
export type WebdavAccountCreated =
	components["schemas"]["WebdavAccountCreated"];
export type WebdavAccountInfo = components["schemas"]["WebdavAccountInfo"];
export type WebdavAccountPage =
	components["schemas"]["OffsetPage_WebdavAccountInfo"];
export type WebdavAccountListQuery = OperationQuery<"list_webdav_accounts">;
export type TeamWebdavAccountListQuery =
	OperationQuery<"list_team_webdav_accounts">;
export type CreateWebdavAccountRequest =
	components["schemas"]["CreateWebdavAccountReq"];
export type TestWebdavConnectionRequest =
	components["schemas"]["TestConnectionReq"];
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
