import type { PolicyFormData } from "./formTypes";

export interface StoragePolicyDialogStep {
	description: string;
	title: string;
}

export interface StoragePolicySummaryItem {
	label: string;
	value: string;
}

export type StoragePolicyFieldChangeHandler = <K extends keyof PolicyFormData>(
	key: K,
	value: PolicyFormData[K],
) => void;
