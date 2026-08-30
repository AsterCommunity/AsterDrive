import type { components } from "@/services/api.generated";
import type {
	CreatePolicyGroupRequest,
	StoragePlacementRuleInfo,
	StoragePolicy,
	StoragePolicyGroup,
} from "@/types/api";

const BYTES_PER_MB = 1024 * 1024;

export interface PolicyGroupRuleTargetForm {
	key: string;
	policyId: string;
	weight: string;
	isEnabled: boolean;
	acceptingNewWrites: boolean;
}

export interface PolicyGroupRuleForm {
	key: string;
	minFileSizeMb: string;
	maxFileSizeMb: string;
	originalMinFileSizeBytes?: number;
	originalMaxFileSizeBytes?: number;
	selectionMode: "first_available" | "weighted_random";
	unavailableBehavior: "next_rule" | "reject";
	targets: PolicyGroupRuleTargetForm[];
}

export interface PolicyGroupFormData {
	name: string;
	description: string;
	isEnabled: boolean;
	isDefault: boolean;
	items: PolicyGroupRuleForm[];
	admission?: components["schemas"]["StorageAdmissionConstraints"];
	executionPreference?: "automatic" | "force_server_stream";
}

export type PolicyGroupPayload = Pick<
	CreatePolicyGroupRequest,
	| "name"
	| "description"
	| "is_enabled"
	| "is_default"
	| "admission"
	| "execution_preference"
	| "rules"
>;

function generateRuleKey() {
	return (
		globalThis.crypto?.randomUUID?.() ??
		`policy-group-rule-${Date.now()}-${Math.random().toString(36).slice(2)}`
	);
}

export function buildPolicyGroupRuleTargetForm(
	policyId?: number | null,
	ruleTarget?: Pick<
		StoragePlacementRuleInfo["targets"][number],
		"policy_id" | "weight" | "is_enabled" | "accepting_new_writes"
	>,
): PolicyGroupRuleTargetForm {
	return {
		key: generateRuleKey(),
		policyId:
			ruleTarget != null
				? String(ruleTarget.policy_id)
				: policyId != null
					? String(policyId)
					: "",
		weight: String(ruleTarget?.weight ?? 100),
		isEnabled: ruleTarget?.is_enabled ?? true,
		acceptingNewWrites: ruleTarget?.accepting_new_writes ?? true,
	};
}

export function bytesToMbInput(bytes: number) {
	if (bytes <= 0) return "";

	const mb = bytes / BYTES_PER_MB;
	return String(mb);
}

export function mbInputToBytes(value: string, originalBytes?: number) {
	const normalized = value.trim();
	if (!normalized) return 0;

	if (
		originalBytes != null &&
		originalBytes > 0 &&
		normalized === bytesToMbInput(originalBytes)
	) {
		return originalBytes;
	}

	const parsed = Number(normalized);
	if (!Number.isFinite(parsed) || parsed <= 0) return 0;

	return Math.round(parsed * BYTES_PER_MB);
}

export function buildPolicyGroupRuleForm(
	policyId?: number | null,
	minFileSize = 0,
	maxFileSize = 0,
	rule?: Pick<
		StoragePlacementRuleInfo,
		"selection_mode" | "unavailable_behavior" | "targets"
	>,
): PolicyGroupRuleForm {
	return {
		key: generateRuleKey(),
		minFileSizeMb: bytesToMbInput(minFileSize),
		maxFileSizeMb: bytesToMbInput(maxFileSize),
		originalMinFileSizeBytes: minFileSize || undefined,
		originalMaxFileSizeBytes: maxFileSize || undefined,
		selectionMode: rule?.selection_mode ?? "first_available",
		unavailableBehavior: rule?.unavailable_behavior ?? "next_rule",
		targets: rule?.targets.length
			? [...rule.targets]
					.sort((a, b) => a.stable_order - b.stable_order)
					.map((target) => buildPolicyGroupRuleTargetForm(null, target))
			: [buildPolicyGroupRuleTargetForm(policyId ?? null)],
	};
}

export function getDefaultPolicyGroupForm(
	policies: Pick<StoragePolicy, "id">[],
): PolicyGroupFormData {
	return {
		name: "",
		description: "",
		isEnabled: true,
		isDefault: false,
		items: [buildPolicyGroupRuleForm(policies[0]?.id ?? null)],
		admission: {
			allowed_extensions: [],
			denied_extensions: [],
			accept_extensionless: true,
			allowed_categories: [],
			denied_categories: [],
			max_file_size: 0,
		},
		executionPreference: "automatic",
	};
}

export function getPolicyGroupForm(
	group: StoragePolicyGroup,
): PolicyGroupFormData {
	return {
		name: group.name,
		description: group.description,
		isEnabled: group.is_enabled,
		isDefault: group.is_default,
		items: [...(group.rules ?? [])]
			.sort((a, b) => a.priority - b.priority)
			.map((rule) =>
				buildPolicyGroupRuleForm(
					null,
					rule.matcher.min_file_size,
					rule.matcher.max_file_size,
					rule,
				),
			),
		admission: group.admission,
		executionPreference: group.execution_preference,
	};
}

export function validatePolicyGroupForm(
	form: PolicyGroupFormData,
	availablePolicyCount: number,
	t: (key: string) => string,
): string | null {
	if (!form.name.trim()) {
		return t("policy_group_name_required");
	}
	if (form.isDefault && !form.isEnabled) {
		return t("policy_group_default_requires_enabled");
	}
	if (availablePolicyCount === 0) {
		return t("policy_group_no_policies_available");
	}
	if (form.items.length === 0) {
		return t("policy_group_rule_required");
	}

	const seenPrimaryPolicyIds = new Set<number>();

	for (const item of form.items) {
		if (item.targets.length === 0) {
			return t("policy_group_rule_target_required");
		}

		const seenTargetPolicyIds = new Set<number>();
		for (const target of item.targets) {
			const policyIdNum = Number(target.policyId);
			if (!Number.isInteger(policyIdNum) || policyIdNum <= 0) {
				return t("policy_group_rule_policy_required");
			}
			if (seenTargetPolicyIds.has(policyIdNum)) {
				return t("policy_group_rule_target_duplicate");
			}
			seenTargetPolicyIds.add(policyIdNum);

			const weight = Number(target.weight);
			if (!Number.isInteger(weight) || weight <= 0) {
				return t("policy_group_target_weight_invalid");
			}
		}

		const primaryPolicyId = Number(item.targets[0].policyId);
		if (seenPrimaryPolicyIds.has(primaryPolicyId)) {
			return t("policy_group_rule_policy_duplicate");
		}
		seenPrimaryPolicyIds.add(primaryPolicyId);

		const min = item.minFileSizeMb.trim() ? Number(item.minFileSizeMb) : 0;
		const max = item.maxFileSizeMb.trim() ? Number(item.maxFileSizeMb) : 0;
		if (!Number.isFinite(min) || !Number.isFinite(max) || min < 0 || max < 0) {
			return t("policy_group_rule_size_invalid");
		}
		if (max > 0 && max <= min) {
			return t("policy_group_rule_range_invalid");
		}
	}

	return null;
}

export function buildPolicyGroupPayload(
	form: PolicyGroupFormData,
): PolicyGroupPayload {
	return {
		name: form.name.trim(),
		description: form.description.trim(),
		is_enabled: form.isEnabled,
		is_default: form.isDefault,
		admission: form.admission ?? {
			allowed_extensions: [],
			denied_extensions: [],
			accept_extensionless: true,
			allowed_categories: [],
			denied_categories: [],
			max_file_size: 0,
		},
		execution_preference: form.executionPreference ?? "automatic",
		rules: form.items.map((item, index) => ({
			name: `Rule ${index + 1}`,
			description: "",
			priority: index + 1,
			is_enabled: true,
			matcher: {
				min_file_size: mbInputToBytes(
					item.minFileSizeMb,
					item.originalMinFileSizeBytes,
				),
				max_file_size: mbInputToBytes(
					item.maxFileSizeMb,
					item.originalMaxFileSizeBytes,
				),
				extensions: [],
				compound_extensions: [],
				extensionless: null,
				categories: [],
			},
			selection_mode: item.selectionMode ?? "first_available",
			unavailable_behavior: item.unavailableBehavior ?? "next_rule",
			targets: item.targets.map((target, targetIndex) => ({
				policy_id: Number(target.policyId),
				weight: Number(target.weight),
				is_enabled: target.isEnabled,
				accepting_new_writes: target.acceptingNewWrites,
				stable_order: targetIndex + 1,
			})),
		})),
	};
}
