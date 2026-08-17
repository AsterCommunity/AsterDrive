import type {
	StorageConnectorDescriptor,
	StorageConnectorPromotionDescriptor,
} from "@/types/api";
import type { ConnectorFormValue, PolicyFormData } from "./formTypes";
import { applyPolicyConnectorTransition } from "./policyFormTransition";

export interface StorageConnectorPromotionCandidate {
	targetDescriptor: StorageConnectorDescriptor;
	promotion: StorageConnectorPromotionDescriptor;
}

export function storageConnectorPromotionKey(
	candidate: StorageConnectorPromotionCandidate,
) {
	return `${candidate.targetDescriptor.connector_id}:${candidate.promotion.promotion_id}`;
}

export function findStorageConnectorPromotionCandidates(
	descriptors: StorageConnectorDescriptor[],
	form: PolicyFormData,
): StorageConnectorPromotionCandidate[] {
	return descriptors.flatMap((targetDescriptor) =>
		(targetDescriptor.promotions ?? [])
			.filter(
				(promotion) =>
					promotion.source_connector_id === form.connector_id &&
					promotionRequirementsMatch(promotion, form.connector_config_values),
			)
			.map((promotion) => ({ targetDescriptor, promotion })),
	);
}

export function applyStorageConnectorPromotion(
	form: PolicyFormData,
	candidate: StorageConnectorPromotionCandidate,
): PolicyFormData {
	const transitioned = applyPolicyConnectorTransition(
		form,
		candidate.targetDescriptor.connector_id,
		candidate.targetDescriptor,
	);
	const connectorConfigValues = {
		...transitioned.connector_config_values,
	};
	const explicitFields = new Set(
		transitioned.connector_config_explicit_fields ?? [],
	);
	for (const mapping of candidate.promotion.config_mappings) {
		const value = form.connector_config_values[mapping.source_field];
		if (value === undefined) {
			continue;
		}
		connectorConfigValues[mapping.target_field] = value;
		explicitFields.add(mapping.target_field);
	}

	const credentialValues = { ...transitioned.credential_values };
	for (const mapping of candidate.promotion.credential_mappings ?? []) {
		const value = form.credential_values[mapping.source_field];
		if (value !== undefined) {
			credentialValues[mapping.target_field] = value;
		}
	}

	return {
		...transitioned,
		connector_config_values: connectorConfigValues,
		connector_config_explicit_fields: [...explicitFields],
		credential_values: credentialValues,
		storage_native_thumbnail_enabled:
			candidate.targetDescriptor.capabilities.storage_native_thumbnail === true
				? form.storage_native_thumbnail_enabled
				: false,
		storage_native_media_metadata_enabled:
			candidate.targetDescriptor.capabilities.storage_native_media_metadata ===
			true
				? form.storage_native_media_metadata_enabled
				: false,
	};
}

function promotionRequirementsMatch(
	promotion: StorageConnectorPromotionDescriptor,
	values: Record<string, ConnectorFormValue>,
) {
	return (promotion.requirements ?? []).every((requirement) => {
		const value = values[requirement.source_field];
		if (typeof value !== "string") {
			return false;
		}
		const matcher = requirement.matcher;
		let matches: boolean;
		switch (matcher.kind) {
			case "string_equals":
				matches = compareText(
					value,
					matcher.value,
					matcher.case_sensitive === true,
					(left, right) => left === right,
				);
				break;
			case "string_suffix":
				matches = compareText(
					value,
					matcher.suffix,
					matcher.case_sensitive === true,
					(left, right) => left.endsWith(right),
				);
				break;
			case "string_prefix":
				matches = compareText(
					value,
					matcher.prefix,
					matcher.case_sensitive === true,
					(left, right) => left.startsWith(right),
				);
				break;
			case "url_host_suffix":
				try {
					matches = new URL(value).hostname
						.toLowerCase()
						.endsWith(matcher.suffix.toLowerCase());
				} catch {
					matches = false;
				}
				break;
		}
		return matches !== (requirement.negate === true);
	});
}

function compareText(
	value: string,
	expected: string,
	caseSensitive: boolean,
	compare: (left: string, right: string) => boolean,
) {
	return caseSensitive
		? compare(value, expected)
		: compare(value.toLowerCase(), expected.toLowerCase());
}
