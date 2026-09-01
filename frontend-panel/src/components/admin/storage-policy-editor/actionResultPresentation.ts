import type {
	StorageConnectorActionDescriptor,
	StoragePolicyActionResult,
} from "@/types/api";

export interface StorageConnectorActionOutputDetail {
	label: string;
	value: string;
}

export function presentStorageConnectorActionOutput(
	action: StorageConnectorActionDescriptor,
	result: StoragePolicyActionResult,
	translate: (key: string) => string,
): StorageConnectorActionOutputDetail[] {
	if (
		!result.ok ||
		result.action_id !== action.action_id ||
		!result.output ||
		Array.isArray(result.output)
	) {
		return [];
	}

	return (action.output_fields ?? []).flatMap((field) => {
		const value = result.output?.[field.name];
		const presented = presentOutputValue(field, value, translate);
		return presented === null
			? []
			: [{ label: translate(field.label_key), value: presented }];
	});
}

function presentOutputValue(
	field: NonNullable<StorageConnectorActionDescriptor["output_fields"]>[number],
	value: unknown,
	translate: (key: string) => string,
) {
	switch (field.value_kind) {
		case "text":
			return typeof value === "string" && value.trim() ? value.trim() : null;
		case "number":
			return typeof value === "number" && Number.isFinite(value)
				? String(value)
				: null;
		case "boolean":
			if (typeof value !== "boolean") return null;
			return value
				? field.true_key
					? translate(field.true_key)
					: null
				: field.false_key
					? translate(field.false_key)
					: null;
		case "string_list": {
			if (
				!Array.isArray(value) ||
				!value.every((item) => typeof item === "string")
			) {
				return null;
			}
			const values = value.map((item) => item.trim()).filter(Boolean);
			return values.length > 0 ? values.join(", ") : null;
		}
	}
}
