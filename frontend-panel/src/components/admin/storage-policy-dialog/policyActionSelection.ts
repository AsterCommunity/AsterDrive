import type { StorageConnectorDescriptor, StoragePolicy } from "@/types/api";
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
