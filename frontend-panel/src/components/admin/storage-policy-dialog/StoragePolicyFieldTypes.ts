import type { DriverType } from "@/types/api";
import type { PolicyFormData } from "./formTypes";

export interface StoragePolicyDriverOption {
	description: string;
	iconName?: "Globe";
	iconSrc?: string;
	type: DriverType;
	title: string;
}

export type Translate = (
	key: string,
	values?: Record<string, number | string>,
) => string;

export type PolicyFieldChange = <K extends keyof PolicyFormData>(
	key: K,
	value: PolicyFormData[K],
) => void;

export interface SharedFieldProps {
	form: PolicyFormData;
	onFieldChange: PolicyFieldChange;
	t: Translate;
}

export interface SelectOption<TValue extends string> {
	label: string;
	value: TValue;
}
