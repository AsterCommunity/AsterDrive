import type { StoragePolicyGroup } from "@/types/api";

export interface PolicyGroupOption {
	disabled?: boolean;
	label: string;
	value: string;
}

export interface UserPasswordErrors {
	confirm?: string;
	password?: string;
}

export function buildPolicyGroupOptions(
	policyGroups: StoragePolicyGroup[],
	selectedPolicyGroupId: number | null,
): PolicyGroupOption[] {
	const options: PolicyGroupOption[] = [];
	for (const group of policyGroups) {
		if (!group.is_enabled || group.rules.length === 0) {
			continue;
		}
		options.push({
			label: group.name,
			value: String(group.id),
		});
	}

	if (
		selectedPolicyGroupId != null &&
		!options.some((option) => option.value === String(selectedPolicyGroupId))
	) {
		const selectedGroup = policyGroups.find(
			(group) => group.id === selectedPolicyGroupId,
		);
		options.unshift({
			label: selectedGroup?.name ?? `#${selectedPolicyGroupId}`,
			value: String(selectedPolicyGroupId),
			disabled: true,
		});
	}

	return options;
}
