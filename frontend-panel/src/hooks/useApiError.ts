import axios from "axios";
import { toast } from "sonner";
import i18n from "@/i18n";
import { ApiError } from "@/services/http";
import {
	ApiSubcode,
	type ApiSubcode as ApiSubcodeType,
	ErrorCode,
	type ErrorCode as ErrorCodeType,
	isApiSubcode,
} from "@/types/api-helpers";

const errorMessageKeys: Partial<Record<ErrorCodeType, string>> = {
	[ErrorCode.RateLimited]: "errors:rate_limited",
	[ErrorCode.MailNotConfigured]: "errors:mail_not_configured",
	[ErrorCode.MailDeliveryFailed]: "errors:mail_delivery_failed",
	[ErrorCode.AuthFailed]: "errors:auth_failed",
	[ErrorCode.TokenExpired]: "errors:token_expired",
	[ErrorCode.TokenInvalid]: "errors:token_invalid",
	[ErrorCode.TokenMissing]: "errors:token_missing",
	[ErrorCode.CredentialsFailed]: "errors:credentials_failed",
	[ErrorCode.MfaFailed]: "errors:mfa_failed",
	[ErrorCode.Forbidden]: "errors:forbidden",
	[ErrorCode.PendingActivation]: "errors:pending_activation",
	[ErrorCode.ContactVerificationInvalid]: "errors:contact_verification_invalid",
	[ErrorCode.ContactVerificationExpired]: "errors:contact_verification_expired",
	[ErrorCode.FileNotFound]: "errors:file_not_found",
	[ErrorCode.FileTooLarge]: "errors:file_too_large",
	[ErrorCode.FileTypeNotAllowed]: "errors:file_type_not_allowed",
	[ErrorCode.FileUploadFailed]: "errors:file_upload_failed",
	[ErrorCode.UploadSessionNotFound]: "errors:upload_session_not_found",
	[ErrorCode.UploadSessionExpired]: "errors:upload_session_expired",
	[ErrorCode.ChunkUploadFailed]: "errors:chunk_upload_failed",
	[ErrorCode.ResourceLocked]: "errors:resource_locked",
	[ErrorCode.PreconditionFailed]: "errors:precondition_failed",
	[ErrorCode.UploadAssembling]: "errors:upload_assembling",
	[ErrorCode.StorageQuotaExceeded]: "errors:storage_quota_exceeded",
	[ErrorCode.StorageAuthFailed]: "errors:storage_auth_failed",
	[ErrorCode.StoragePermissionDenied]: "errors:storage_permission_denied",
	[ErrorCode.StorageMisconfigured]: "errors:storage_misconfigured",
	[ErrorCode.StorageObjectNotFound]: "errors:storage_not_found",
	[ErrorCode.StorageRateLimited]: "errors:storage_rate_limited",
	[ErrorCode.StorageTransientFailure]: "errors:storage_transient_failure",
	[ErrorCode.StoragePreconditionFailed]: "errors:storage_precondition_failed",
	[ErrorCode.StorageOperationUnsupported]:
		"errors:storage_operation_unsupported",
	[ErrorCode.FolderNotFound]: "errors:folder_not_found",
	[ErrorCode.ShareNotFound]: "errors:share_not_found",
	[ErrorCode.ShareExpired]: "errors:share_expired",
	[ErrorCode.SharePasswordRequired]: "errors:share_password_required",
	[ErrorCode.ShareDownloadLimitReached]: "errors:share_download_limit_reached",
};

const errorSubcodeKeys: Partial<Record<ApiSubcodeType, string>> = {
	[ApiSubcode.AuthUsernameExists]: "errors:auth_username_exists",
	[ApiSubcode.AuthEmailExists]: "errors:auth_email_exists",
	[ApiSubcode.AuthIdentifierExists]: "errors:auth_identifier_exists",
	[ApiSubcode.AuthAdminRequired]: "errors:auth_admin_required",
	[ApiSubcode.AuthAccountDisabled]: "errors:auth_account_disabled",
	[ApiSubcode.AuthRequestSourceUntrusted]:
		"errors:auth_request_source_untrusted",
	[ApiSubcode.AuthRequestOriginUntrusted]:
		"errors:auth_request_origin_untrusted",
	[ApiSubcode.AuthRequestRefererUntrusted]:
		"errors:auth_request_referer_untrusted",
	[ApiSubcode.AuthRequestSourceMissing]: "errors:auth_request_source_missing",
	[ApiSubcode.AuthSessionUserMismatch]: "errors:auth_session_user_mismatch",
	[ApiSubcode.AuthCsrfCookieMissing]: "errors:auth_csrf_cookie_missing",
	[ApiSubcode.AuthCsrfHeaderMissing]: "errors:auth_csrf_header_missing",
	[ApiSubcode.AuthCsrfTokenInvalid]: "errors:auth_csrf_token_invalid",
	[ApiSubcode.AuthRegistrationDisabled]: "errors:auth_registration_disabled",
	[ApiSubcode.AuthMfaFlowInvalid]: "errors:auth_mfa_flow_invalid",
	[ApiSubcode.AuthMfaFlowExpired]: "errors:auth_mfa_flow_expired",
	[ApiSubcode.AuthMfaCodeInvalid]: "errors:auth_mfa_code_invalid",
	[ApiSubcode.AuthMfaAttemptsExceeded]: "errors:auth_mfa_attempts_exceeded",
	[ApiSubcode.AuthMfaFactorRequired]: "errors:auth_mfa_factor_required",
	[ApiSubcode.AuthMfaFactorAlreadyExists]:
		"errors:auth_mfa_factor_already_exists",
	[ApiSubcode.AuthMfaRecoveryCodeUsed]: "errors:auth_mfa_recovery_code_used",
	[ApiSubcode.FileEtagMismatch]: "errors:file_etag_mismatch",
	[ApiSubcode.FileNameConflict]: "errors:file_name_conflict",
	[ApiSubcode.FolderNameConflict]: "errors:folder_name_conflict",
	[ApiSubcode.UploadFieldReadFailed]: "errors:upload_field_read_failed",
	[ApiSubcode.UploadRequestBodyReadFailed]:
		"errors:upload_request_body_read_failed",
	[ApiSubcode.UploadRequestBodySizeOverflow]:
		"errors:upload_request_body_size_overflow",
	[ApiSubcode.UploadRequestSizeMismatch]: "errors:upload_request_size_mismatch",
	[ApiSubcode.UploadTempDirCreateFailed]:
		"errors:upload_temp_dir_create_failed",
	[ApiSubcode.UploadTempFileCreateFailed]:
		"errors:upload_temp_file_create_failed",
	[ApiSubcode.UploadTempFileWriteFailed]:
		"errors:upload_temp_file_write_failed",
	[ApiSubcode.UploadTempFileFlushFailed]:
		"errors:upload_temp_file_flush_failed",
	[ApiSubcode.UploadLocalStagingPathResolveFailed]:
		"errors:upload_local_staging_path_resolve_failed",
	[ApiSubcode.UploadLocalStagingDirCreateFailed]:
		"errors:upload_local_staging_dir_create_failed",
	[ApiSubcode.UploadLocalStagingFileCreateFailed]:
		"errors:upload_local_staging_file_create_failed",
	[ApiSubcode.UploadLocalStagingWriteFailed]:
		"errors:upload_local_staging_write_failed",
	[ApiSubcode.UploadLocalStagingFlushFailed]:
		"errors:upload_local_staging_flush_failed",
	[ApiSubcode.UploadBodySizeOverflow]: "errors:upload_body_size_overflow",
	[ApiSubcode.UploadEmptyFile]: "errors:upload_empty_file",
	[ApiSubcode.UploadDirectRelayWriteFailed]:
		"errors:upload_direct_relay_write_failed",
	[ApiSubcode.UploadDirectRelayShutdownFailed]:
		"errors:upload_direct_relay_shutdown_failed",
	[ApiSubcode.UploadDirectRelayTaskFailed]:
		"errors:upload_direct_relay_task_failed",
	[ApiSubcode.UploadDeclaredSizeInvalid]: "errors:upload_declared_size_invalid",
	[ApiSubcode.UploadHashTempOpenFailed]: "errors:upload_hash_temp_open_failed",
	[ApiSubcode.UploadHashTempReadFailed]: "errors:upload_hash_temp_read_failed",
	[ApiSubcode.UploadChunkTransportMismatch]:
		"errors:upload_chunk_transport_mismatch",
	[ApiSubcode.UploadChunkSessionInvalid]: "errors:upload_chunk_session_invalid",
	[ApiSubcode.UploadChunkNumberOutOfRange]:
		"errors:upload_chunk_number_out_of_range",
	[ApiSubcode.UploadChunkSizeMismatch]: "errors:upload_chunk_size_mismatch",
	[ApiSubcode.UploadChunkPersistFailed]: "errors:upload_chunk_persist_failed",
	[ApiSubcode.UploadStatusConflict]: "errors:upload_status_conflict",
	[ApiSubcode.UploadCompletedFileMissing]:
		"errors:upload_completed_file_missing",
	[ApiSubcode.UploadPreviousFailure]: "errors:upload_previous_failure",
	[ApiSubcode.UploadPartsRequired]: "errors:upload_parts_required",
	[ApiSubcode.UploadIncompleteChunks]: "errors:upload_incomplete_chunks",
	[ApiSubcode.UploadIncompleteParts]: "errors:upload_incomplete_parts",
	[ApiSubcode.UploadMissingPart]: "errors:upload_missing_part",
	[ApiSubcode.UploadTempObjectMissing]: "errors:upload_temp_object_missing",
	[ApiSubcode.UploadTempObjectSizeMismatch]:
		"errors:upload_temp_object_size_mismatch",
	[ApiSubcode.UploadSessionCorrupted]: "errors:upload_session_corrupted",
	[ApiSubcode.UploadChunkRelayFailed]: "errors:upload_chunk_relay_failed",
	[ApiSubcode.UploadAssemblyIoFailed]: "errors:upload_assembly_io_failed",
	[ApiSubcode.UploadAssemblySizeOverflow]:
		"errors:upload_assembly_size_overflow",
	[ApiSubcode.StorageAuth]: "errors:storage_auth_failed",
	[ApiSubcode.StoragePermission]: "errors:storage_permission_denied",
	[ApiSubcode.StorageMisconfigured]: "errors:storage_misconfigured",
	[ApiSubcode.StorageNotFound]: "errors:storage_not_found",
	[ApiSubcode.StorageRateLimited]: "errors:storage_rate_limited",
	[ApiSubcode.StorageTransient]: "errors:storage_transient_failure",
	[ApiSubcode.StoragePrecondition]: "errors:storage_precondition_failed",
	[ApiSubcode.StorageUnsupported]: "errors:storage_operation_unsupported",
	[ApiSubcode.TaskLeaseLost]: "errors:task_lease_lost",
	[ApiSubcode.TaskLeaseRenewalTimedOut]: "errors:task_lease_renewal_timed_out",
	[ApiSubcode.ThumbnailFormatGuessFailed]:
		"errors:thumbnail_format_guess_failed",
	[ApiSubcode.ThumbnailDecodeFailed]: "errors:thumbnail_decode_failed",
	[ApiSubcode.ThumbnailEncodeFailed]: "errors:thumbnail_encode_failed",
	[ApiSubcode.ThumbnailProcessorUnavailable]:
		"errors:thumbnail_processor_unavailable",
	[ApiSubcode.ThumbnailRenderFailed]: "errors:thumbnail_render_failed",
	[ApiSubcode.ThumbnailOutputInvalid]: "errors:thumbnail_output_invalid",
	[ApiSubcode.ThumbnailTaskPanicked]: "errors:thumbnail_task_panicked",
	[ApiSubcode.ThumbnailSourceTooLarge]: "errors:thumbnail_source_too_large",
	[ApiSubcode.ThumbnailSourceTempCreateFailed]:
		"errors:thumbnail_source_temp_create_failed",
	[ApiSubcode.ThumbnailSourceStreamFailed]:
		"errors:thumbnail_source_stream_failed",
	[ApiSubcode.ThumbnailSourceTempFlushFailed]:
		"errors:thumbnail_source_temp_flush_failed",
	[ApiSubcode.ThumbnailSourceTempCopyFailed]:
		"errors:thumbnail_source_temp_copy_failed",
	[ApiSubcode.AvatarFileRequired]: "errors:avatar_file_required",
	[ApiSubcode.AvatarUploadReadFailed]: "errors:avatar_upload_read_failed",
	[ApiSubcode.AvatarProcessorUnavailable]:
		"errors:avatar_processor_unavailable",
	[ApiSubcode.AvatarEmptyImage]: "errors:avatar_empty_image",
	[ApiSubcode.AvatarRenderFailed]: "errors:avatar_render_failed",
	[ApiSubcode.AvatarOutputInvalid]: "errors:avatar_output_invalid",
	[ApiSubcode.MasterBindingDisabled]: "errors:master_binding_disabled",
	[ApiSubcode.ManagedIngressBindingMismatch]:
		"errors:managed_ingress_binding_mismatch",
	[ApiSubcode.ManagedIngressDefaultDeleteRequiresReplacement]:
		"errors:managed_ingress_default_delete_requires_replacement",
	[ApiSubcode.ManagedIngressDefaultError]:
		"errors:managed_ingress_default_error",
	[ApiSubcode.ManagedIngressDefaultMissing]:
		"errors:managed_ingress_default_missing",
	[ApiSubcode.ManagedIngressDefaultNotApplied]:
		"errors:managed_ingress_default_not_applied",
	[ApiSubcode.ManagedIngressRequired]: "errors:managed_ingress_required",
	[ApiSubcode.ManagedIngressDefaultUpdateRequiresReplacement]:
		"errors:managed_ingress_default_update_requires_replacement",
	[ApiSubcode.ManagedIngressDriverUnsupported]:
		"errors:managed_ingress_driver_unsupported",
	[ApiSubcode.ManagedIngressLocalPathInvalid]:
		"errors:managed_ingress_local_path_invalid",
	[ApiSubcode.ManagedIngressSinglePrimaryRequired]:
		"errors:managed_ingress_single_primary_required",
	[ApiSubcode.RemoteNodeDisabled]: "errors:remote_node_disabled",
	[ApiSubcode.LockNotOwner]: "errors:lock_not_owner",
	[ApiSubcode.ShareScopeDenied]: "errors:share_scope_denied",
	[ApiSubcode.TeamMemberExists]: "errors:team_member_exists",
	[ApiSubcode.TeamNotMember]: "errors:team_not_member",
	[ApiSubcode.TeamOwnerRequired]: "errors:team_owner_required",
	[ApiSubcode.TeamAdminOrOwnerRequired]: "errors:team_admin_or_owner_required",
	[ApiSubcode.WorkspaceScopeDenied]: "errors:workspace_scope_denied",
	[ApiSubcode.WebdavUsernameExists]: "errors:webdav_username_exists",
	[ApiSubcode.ExternalAuthProviderDisabled]:
		"errors:external_auth_provider_disabled",
	[ApiSubcode.ExternalAuthPolicyDenied]: "errors:external_auth_policy_denied",
	[ApiSubcode.WopiMaxExpectedSizeExceeded]:
		"errors:wopi_max_expected_size_exceeded",
	[ApiSubcode.WopiPublicSiteUrlRequired]:
		"errors:wopi_public_site_url_required",
	[ApiSubcode.WopiAppDisabled]: "errors:wopi_app_disabled",
	[ApiSubcode.WopiRequestOriginUntrusted]:
		"errors:wopi_request_origin_untrusted",
	[ApiSubcode.WopiRequestRefererUntrusted]:
		"errors:wopi_request_referer_untrusted",
	[ApiSubcode.RemoteNodeUniqueConflict]: "errors:remote_node_unique_conflict",
	[ApiSubcode.ValidationRequestOriginInvalid]:
		"errors:validation_request_origin_invalid",
	[ApiSubcode.ValidationRequestRefererInvalid]:
		"errors:validation_request_referer_invalid",
	[ApiSubcode.ValidationRequestHostInvalid]:
		"errors:validation_request_host_invalid",
	[ApiSubcode.ValidationRequestSchemeInvalid]:
		"errors:validation_request_scheme_invalid",
	[ApiSubcode.ValidationRequestHeaderValueInvalid]:
		"errors:validation_request_header_value_invalid",
	[ApiSubcode.ValidationSystemAlreadyInitialized]:
		"errors:validation_system_already_initialized",
};

function getErrorCode(error: unknown): string | undefined {
	if (typeof error !== "object" || error === null || !("code" in error)) {
		return undefined;
	}
	return typeof error.code === "string" ? error.code : undefined;
}

function getTrimmedErrorMessage(error: Error): string {
	return error.message.trim();
}

function getTransportErrorMessageKey(error: unknown): string | null {
	const code = getErrorCode(error);
	if (code === "ERR_CANCELED") {
		return null;
	}

	const message =
		error instanceof Error ? getTrimmedErrorMessage(error) : undefined;
	const normalizedMessage = message?.toLowerCase();

	const timedOut =
		code === "ECONNABORTED" ||
		code === "ETIMEDOUT" ||
		normalizedMessage?.includes("timeout") === true;
	if (timedOut) {
		return "errors:request_timeout";
	}

	if (axios.isAxiosError(error) && !error.response) {
		return "errors:network_error";
	}

	if (
		message === "Network Error" ||
		normalizedMessage === "network error" ||
		message === "Failed to fetch" ||
		message === "Load failed"
	) {
		return "errors:network_error";
	}

	return null;
}

export function getApiErrorMessage(error: unknown) {
	if (error instanceof ApiError) {
		const subcodeKey =
			error.subcode && isApiSubcode(error.subcode)
				? errorSubcodeKeys[error.subcode]
				: undefined;
		const key = subcodeKey ?? errorMessageKeys[error.code];
		if (key) {
			return i18n.t(key);
		}
		const message = error.message.trim();
		return message || i18n.t("errors:unexpected_error");
	}

	const transportErrorKey = getTransportErrorMessageKey(error);
	if (transportErrorKey) {
		return i18n.t(transportErrorKey);
	}

	if (error instanceof Error) {
		const message = getTrimmedErrorMessage(error);
		return message || i18n.t("errors:unexpected_error");
	}

	return i18n.t("errors:unexpected_error");
}

export function handleApiError(error: unknown) {
	toast.error(getApiErrorMessage(error));
}
