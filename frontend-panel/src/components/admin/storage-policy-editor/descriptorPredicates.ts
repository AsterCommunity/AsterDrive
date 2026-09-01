import type {
	StorageConnectorActionId,
	StorageConnectorActionKind,
	StorageConnectorDescriptor,
	StorageConnectorFieldDescriptor,
} from "@/types/api";

type StorageConnectorSelectDataSource = NonNullable<
	NonNullable<StorageConnectorFieldDescriptor["select"]>["data_source"]
>;

export function descriptorHasField(
	descriptor: StorageConnectorDescriptor | null | undefined,
	fieldName: string,
) {
	return descriptor?.fields.some((field) => field.name === fieldName) ?? false;
}

export function findConnectorFieldByDataSource(
	descriptor: StorageConnectorDescriptor | null | undefined,
	dataSource: StorageConnectorSelectDataSource,
) {
	return (
		descriptor?.fields.find(
			(field) => field.select?.data_source === dataSource,
		) ?? null
	);
}

export function supportsRemoteNodeBinding(
	descriptor: StorageConnectorDescriptor | null | undefined,
) {
	return descriptor?.capabilities.remote_node_binding === true;
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
	actionId: StorageConnectorActionId,
	kind?: StorageConnectorActionKind,
) {
	return findStorageConnectorAction(descriptor, actionId, kind) != null;
}

export function findStorageConnectorAction(
	descriptor: StorageConnectorDescriptor | null | undefined,
	actionId: StorageConnectorActionId,
	kind?: StorageConnectorActionKind,
) {
	return descriptor?.actions.find(
		(action) =>
			action.action_id === actionId &&
			(kind === undefined || action.kind === kind),
	);
}

export function supportsStorageConnectorCustomAction(
	descriptor: StorageConnectorDescriptor | null | undefined,
	actionId: StorageConnectorActionId,
) {
	return supportsStorageConnectorAction(descriptor, actionId, "custom");
}
