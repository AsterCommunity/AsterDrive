import type {
	StorageConnectorActionKind,
	StorageConnectorAffordanceAction,
	StorageConnectorDescriptor,
	StoragePolicyExecutableAction,
} from "@/types/api";

export function descriptorHasField(
	descriptor: StorageConnectorDescriptor | null | undefined,
	fieldName: string,
) {
	return descriptor?.fields.some((field) => field.name === fieldName) ?? false;
}

export function descriptorHasPolicyOptionField(
	descriptor: StorageConnectorDescriptor | null | undefined,
	fieldName: string,
) {
	return (
		descriptor?.fields.some(
			(field) => field.scope === "connector_config" && field.name === fieldName,
		) ?? false
	);
}

export function descriptorHasConnectionField(
	descriptor: StorageConnectorDescriptor | null | undefined,
	fieldName: string,
) {
	return (
		descriptor?.fields.some(
			(field) => field.scope === "connector_config" && field.name === fieldName,
		) ?? false
	);
}

export function supportsObjectStorageConnection(
	descriptor: StorageConnectorDescriptor | null | undefined,
) {
	return (
		descriptorHasConnectionField(descriptor, "endpoint") &&
		descriptorHasConnectionField(descriptor, "bucket") &&
		descriptor?.upload_workflows.object_multipart_upload === true
	);
}

export function supportsStaticSecretConnection(
	descriptor: StorageConnectorDescriptor | null | undefined,
) {
	return (
		descriptor?.credential_mode === "static_secret" &&
		descriptorHasConnectionField(descriptor, "endpoint") &&
		descriptor.fields.some((field) => field.scope === "static_credential")
	);
}

export function supportsRemoteNodeBinding(
	descriptor: StorageConnectorDescriptor | null | undefined,
) {
	return descriptor?.capabilities.remote_node_binding === true;
}

export function supportsObjectStorageTransferStrategy(
	descriptor: StorageConnectorDescriptor | null | undefined,
) {
	return descriptor?.capabilities.object_storage_transfer_strategy === true;
}

export function supportsOneDrivePolicyOptions(
	descriptor: StorageConnectorDescriptor | null | undefined,
) {
	return descriptorHasPolicyOptionField(descriptor, "account_mode");
}

export function supportsContentDedupPolicyOption(
	descriptor: StorageConnectorDescriptor | null | undefined,
) {
	return descriptorHasPolicyOptionField(descriptor, "content_dedup");
}

export function supportsApplicationCredentials(
	descriptor: StorageConnectorDescriptor | null | undefined,
) {
	return (
		descriptor?.fields.some(
			(field) => field.scope === "authorization_application",
		) ?? false
	);
}

export function supportsStorageCredentialLifecycle(
	descriptor: StorageConnectorDescriptor | null | undefined,
) {
	return (
		supportsStorageAuthorizationAction(descriptor) ||
		supportsCredentialValidationAction(descriptor) ||
		descriptor?.credential_mode === "oauth_delegated" ||
		descriptor?.authorization_provider != null
	);
}

export function supportsMicrosoftGraphApplicationConfig(
	descriptor: StorageConnectorDescriptor | null | undefined,
) {
	return (
		descriptor?.fields.some(
			(field) =>
				field.scope === "authorization_application" &&
				field.name === "client_id",
		) ?? false
	);
}

export function supportsStorageNativeProcessing(
	descriptor?: StorageConnectorDescriptor | null,
) {
	if (descriptor) {
		return (
			descriptor.capabilities.storage_native_thumbnail ||
			descriptor.capabilities.storage_native_media_metadata
		);
	}
	return false;
}

export function supportsDraftConnectionTest(
	descriptor?: StorageConnectorDescriptor | null,
) {
	return supportsStorageConnectorAction(
		descriptor,
		"test_draft_connection",
		"connection_test",
	);
}

export function supportsSavedConnectionTest(
	descriptor?: StorageConnectorDescriptor | null,
) {
	return supportsStorageConnectorAction(
		descriptor,
		"test_saved_connection",
		"connection_test",
	);
}

export function supportsStorageAuthorizationAction(
	descriptor?: StorageConnectorDescriptor | null,
) {
	return supportsStorageConnectorAction(
		descriptor,
		"start_authorization",
		"authorization",
	);
}

export function supportsCredentialValidationAction(
	descriptor?: StorageConnectorDescriptor | null,
) {
	return supportsStorageConnectorAction(
		descriptor,
		"validate_credential",
		"credential_validation",
	);
}

export function supportsStorageConnectorAction(
	descriptor: StorageConnectorDescriptor | null | undefined,
	action: StorageConnectorAffordanceAction,
	kind?: StorageConnectorActionKind,
) {
	return descriptor
		? descriptor.actions.some(
				(actionDescriptor) =>
					actionDescriptor.affordance_action === action &&
					(kind === undefined || actionDescriptor.kind === kind),
			)
		: false;
}

export function supportsStoragePolicyAction(
	descriptor: StorageConnectorDescriptor | null | undefined,
	action: StoragePolicyExecutableAction,
) {
	return descriptor
		? descriptor.actions.some(
				(actionDescriptor) =>
					actionDescriptor.policy_action === action &&
					actionDescriptor.kind === "policy_action",
			)
		: false;
}
