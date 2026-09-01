import type {
	StorageConnectorActionDescriptor,
	StorageConnectorDescriptor,
	StoragePolicy,
} from "@/types/api";
import {
	getPolicyConnectionTestKey,
	hasConnectionFieldChanges,
} from "./connectionNormalization";
import {
	supportsDraftConnectionTest,
	supportsSavedConnectionTest,
} from "./descriptorPredicates";
import type { PolicyFormData } from "./formTypes";

export type StoragePolicyActionValueSource = "draft" | "saved";
export type StoragePolicyConnectionTestMode =
	| StoragePolicyActionValueSource
	| "unsupported";
export type StorageConnectorCustomActionExecutionMode =
	| StoragePolicyActionValueSource
	| "save_first"
	| "unsupported";

interface StoragePolicyActionSelectionInput {
	descriptor?: StorageConnectorDescriptor | null;
	editingId: number | null;
	editingPolicy: StoragePolicy | null;
	form: PolicyFormData;
}

export function selectStoragePolicyActionValueSource({
	descriptor,
	editingId,
	editingPolicy,
	form,
}: StoragePolicyActionSelectionInput): StoragePolicyActionValueSource {
	return editingId === null ||
		hasConnectionFieldChanges(form, editingPolicy, descriptor)
		? "draft"
		: "saved";
}

export function selectStoragePolicyConnectionTestMode(
	input: StoragePolicyActionSelectionInput,
): StoragePolicyConnectionTestMode {
	const source = selectStoragePolicyActionValueSource(input);
	if (source === "draft") {
		return supportsDraftConnectionTest(input.descriptor)
			? "draft"
			: "unsupported";
	}
	return supportsSavedConnectionTest(input.descriptor)
		? "saved"
		: "unsupported";
}

/// Select the endpoint for one connector-owned custom action.
///
/// The decision is purely descriptor-driven. A draft-only action can execute
/// against an unchanged saved policy, while a saved-only action reports
/// `save_first` whenever current form values are not persisted yet.
export function selectStorageConnectorCustomActionExecutionMode(
	action: StorageConnectorActionDescriptor,
	valueSource: StoragePolicyActionValueSource,
	editingId: number | null,
): StorageConnectorCustomActionExecutionMode {
	const supportsDraft =
		action.endpoints?.includes("execute_draft_storage_policy_action") === true;
	const supportsSaved =
		action.endpoints?.includes("execute_saved_storage_policy_action") === true;

	if (valueSource === "draft") {
		if (supportsDraft) {
			return "draft";
		}
		return supportsSaved ? "save_first" : "unsupported";
	}
	if (editingId !== null && supportsSaved) {
		return "saved";
	}
	return supportsDraft ? "draft" : "unsupported";
}

export function shouldRunPolicyConnectionSaveTest({
	descriptor,
	editingId,
	editingPolicy,
	form,
	validatedConnectionKey,
}: StoragePolicyActionSelectionInput & {
	validatedConnectionKey: string | null;
}) {
	if (!supportsDraftConnectionTest(descriptor)) {
		return false;
	}

	if (
		editingId !== null &&
		!hasConnectionFieldChanges(form, editingPolicy, descriptor)
	) {
		return false;
	}

	return (
		validatedConnectionKey !== getPolicyConnectionTestKey(form, descriptor)
	);
}
