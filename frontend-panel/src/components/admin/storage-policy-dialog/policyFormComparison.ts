import type { StorageConnectorDescriptor, StoragePolicy } from "@/types/api";
import { microsoftGraphCredentials } from "./applicationCredentials";
import { normalizePolicyForm } from "./connectionNormalization";
import {
	supportsMicrosoftGraphApplicationConfig,
	supportsOneDrivePolicyOptions,
} from "./descriptorPredicates";
import { getPolicyForm, type PolicyFormData } from "./formTypes";

function policyFormValueEquals(left: unknown, right: unknown): boolean {
	if (Object.is(left, right)) {
		return true;
	}
	if (Array.isArray(left) || Array.isArray(right)) {
		if (!Array.isArray(left) || !Array.isArray(right)) {
			return false;
		}
		return (
			left.length === right.length &&
			left.every((item, index) => policyFormValueEquals(item, right[index]))
		);
	}
	if (
		left === null ||
		right === null ||
		typeof left !== "object" ||
		typeof right !== "object"
	) {
		return false;
	}

	const leftRecord = left as Record<string, unknown>;
	const rightRecord = right as Record<string, unknown>;
	const leftKeys = Object.keys(leftRecord);
	if (leftKeys.length !== Object.keys(rightRecord).length) {
		return false;
	}

	return leftKeys.every(
		(key) =>
			Object.hasOwn(rightRecord, key) &&
			policyFormValueEquals(leftRecord[key], rightRecord[key]),
	);
}

export function policyFormHasUnsavedChanges(
	form: PolicyFormData,
	policy: StoragePolicy | null,
	descriptor?: StorageConnectorDescriptor | null,
) {
	if (!policy) {
		return false;
	}

	const comparableForm = normalizePolicyComparableForm(form, descriptor);
	const comparablePolicyForm = normalizePolicyComparableForm(
		getPolicyForm(policy),
		descriptor,
	);

	return !policyFormValueEquals(comparableForm, comparablePolicyForm);
}

function normalizePolicyComparableForm(
	form: PolicyFormData,
	descriptor?: StorageConnectorDescriptor | null,
) {
	const normalized = normalizePolicyForm(form, descriptor);
	const usesMicrosoftGraph =
		descriptor != null
			? supportsOneDrivePolicyOptions(descriptor) ||
				supportsMicrosoftGraphApplicationConfig(descriptor)
			: hasMicrosoftGraphFormFields(normalized);

	if (!usesMicrosoftGraph) {
		const {
			onedrive_account_mode: _accountMode,
			onedrive_cloud: _cloud,
			onedrive_drive_id: _driveId,
			onedrive_group_id: _groupId,
			onedrive_root_item_id: _rootItemId,
			onedrive_site_id: _siteId,
			onedrive_tenant: _tenant,
			application_credentials: _applicationCredentials,
			...comparable
		} = normalized;
		return comparable;
	}

	const microsoftGraph = microsoftGraphCredentials(normalized);
	if (microsoftGraph.client_id.trim() || microsoftGraph.client_secret.trim()) {
		return normalized;
	}

	const { application_credentials: _applicationCredentials, ...comparable } =
		normalized;
	return comparable;
}

function hasMicrosoftGraphFormFields(form: PolicyFormData) {
	const microsoftGraph = microsoftGraphCredentials(form);
	return (
		Boolean(form.onedrive_account_mode) ||
		Boolean(form.onedrive_cloud) ||
		Boolean(form.onedrive_tenant.trim()) ||
		Boolean(form.onedrive_drive_id.trim()) ||
		Boolean(form.onedrive_root_item_id.trim()) ||
		Boolean(microsoftGraph.client_id.trim()) ||
		Boolean(microsoftGraph.client_secret.trim()) ||
		Boolean(microsoftGraph.scopes.trim())
	);
}
