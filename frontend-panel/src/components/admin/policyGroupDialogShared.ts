import type { components } from "@/services/api.generated";
import type {
	CreatePolicyGroupRequest,
	StoragePlacementRuleInfo,
	StoragePolicy,
	StoragePolicyGroup,
} from "@/types/api";

const BYTES_PER_MB = 1024 * 1024;

export interface PolicyGroupRuleForm {
	key: string;
	policyId: string;
	priority: string;
	minFileSizeMb: string;
	maxFileSizeMb: string;
	originalMinFileSizeBytes?: number;
	originalMaxFileSizeBytes?: number;
	selectionMode: "first_available" | "weighted_random";
	unavailableBehavior: "next_rule" | "reject";
	targets: Array<{
		policyId: string;
		weight: number;
		isEnabled: boolean;
		acceptingNewWrites: boolean;
		stableOrder: number;
	}>;
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
	priority = 1,
	minFileSize = 0,
	maxFileSize = 0,
	rule?: Pick<
		StoragePlacementRuleInfo,
		"selection_mode" | "unavailable_behavior" | "targets"
	>,
): PolicyGroupRuleForm {
	return {
		key: generateRuleKey(),
		policyId: policyId != null ? String(policyId) : "",
		priority: String(priority),
		minFileSizeMb: bytesToMbInput(minFileSize),
		maxFileSizeMb: bytesToMbInput(maxFileSize),
		originalMinFileSizeBytes: minFileSize || undefined,
		originalMaxFileSizeBytes: maxFileSize || undefined,
		selectionMode: rule?.selection_mode ?? "first_available",
		unavailableBehavior: rule?.unavailable_behavior ?? "next_rule",
		targets:
			rule?.targets.map((target) => ({
				policyId: String(target.policy_id),
				weight: target.weight,
				isEnabled: target.is_enabled,
				acceptingNewWrites: target.accepting_new_writes,
				stableOrder: target.stable_order,
			})) ??
			(policyId != null
				? [
						{
							policyId: String(policyId),
							weight: 100,
							isEnabled: true,
							acceptingNewWrites: true,
							stableOrder: 1,
						},
					]
				: []),
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
		items: (group.rules ?? []).map((rule) =>
			buildPolicyGroupRuleForm(
				rule.targets[0]?.policy_id ?? null,
				rule.priority,
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

	const seenPolicyIds = new Set<number>();
	const seenPriorities = new Set<number>();

	for (const item of form.items) {
		const policyIdNum = Number(item.policyId);
		if (!Number.isInteger(policyIdNum) || policyIdNum <= 0) {
			return t("policy_group_rule_policy_required");
		}

		const priority = Number(item.priority);
		if (!Number.isInteger(priority) || priority <= 0) {
			return t("policy_group_rule_priority_invalid");
		}
		if (seenPolicyIds.has(policyIdNum)) {
			return t("policy_group_rule_policy_duplicate");
		}
		if (seenPriorities.has(priority)) {
			return t("policy_group_rule_priority_duplicate");
		}

		seenPolicyIds.add(policyIdNum);
		seenPriorities.add(priority);

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
	const sortedForms = [...form.items].sort(
		(a, b) => Number(a.priority) - Number(b.priority),
	);
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
		rules: sortedForms.map((item, index) => ({
			name: `Rule ${index + 1}`,
			description: "",
			priority: Number(item.priority),
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
			selection_mode: sortedForms[index].selectionMode ?? "first_available",
			unavailable_behavior:
				sortedForms[index].unavailableBehavior ?? "next_rule",
			targets: (sortedForms[index].targets?.length
				? sortedForms[index].targets
				: [
						{
							policyId: item.policyId,
							weight: 100,
							isEnabled: true,
							acceptingNewWrites: true,
							stableOrder: 1,
						},
					]
			).map((target) => ({
				policy_id: Number(target.policyId),
				weight: target.weight,
				is_enabled: target.isEnabled,
				accepting_new_writes: target.acceptingNewWrites,
				stable_order: target.stableOrder,
			})),
		})),
	};
}
