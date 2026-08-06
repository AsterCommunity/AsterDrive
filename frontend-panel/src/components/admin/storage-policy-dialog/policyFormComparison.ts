import type { StorageConnectorDescriptor, StoragePolicy } from "@/types/api";
import { normalizePolicyForm } from "./connectionNormalization";
import { getPolicyForm, type PolicyFormData } from "./formTypes";

export function policyFormHasUnsavedChanges(
	form: PolicyFormData,
	policy: StoragePolicy | null,
	descriptor?: StorageConnectorDescriptor | null,
) {
	if (!policy) {
		return false;
	}

	const current = comparableForm(normalizePolicyForm(form, descriptor));
	const saved = comparableForm(
		normalizePolicyForm(getPolicyForm(policy), descriptor),
	);
	return !valuesEqual(current, saved);
}

function comparableForm(form: PolicyFormData) {
	return {
		...form,
		// Persisted secrets are intentionally never returned to the form. Empty
		// credential inputs therefore mean "keep saved credential", not a change.
		credential_values: Object.fromEntries(
			Object.entries(form.credential_values).filter(
				([, value]) => value !== "",
			),
		),
	};
}

function valuesEqual(left: unknown, right: unknown): boolean {
	if (Object.is(left, right)) {
		return true;
	}
	if (Array.isArray(left) || Array.isArray(right)) {
		return (
			Array.isArray(left) &&
			Array.isArray(right) &&
			left.length === right.length &&
			left.every((item, index) => valuesEqual(item, right[index]))
		);
	}
	if (!isRecord(left) || !isRecord(right)) {
		return false;
	}
	const leftKeys = Object.keys(left);
	const rightKeys = Object.keys(right);
	return (
		leftKeys.length === rightKeys.length &&
		leftKeys.every(
			(key) => Object.hasOwn(right, key) && valuesEqual(left[key], right[key]),
		)
	);
}

function isRecord(value: unknown): value is Record<string, unknown> {
	return typeof value === "object" && value !== null && !Array.isArray(value);
}
