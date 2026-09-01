import type {
	StorageConnectorDescriptor,
	StorageConnectorFieldDescriptor,
	StorageConnectorFieldValue,
} from "@/types/api";
import type { ConnectorFormValue, PolicyFormData } from "./formTypes";

type FieldValues = Record<string, ConnectorFormValue | undefined>;

export function connectorFieldConditionsMatch(
	conditions: StorageConnectorFieldDescriptor["visible_when"],
	values: FieldValues,
) {
	return (conditions ?? []).every(
		(condition) => values[condition.field] === condition.value,
	);
}

export function isConnectorFieldVisible(
	field: StorageConnectorFieldDescriptor,
	values: FieldValues,
) {
	return connectorFieldConditionsMatch(field.visible_when, values);
}

export function isConnectorFieldRequired(
	field: StorageConnectorFieldDescriptor,
	values: FieldValues,
) {
	return (
		field.required ||
		((field.required_when?.length ?? 0) > 0 &&
			connectorFieldConditionsMatch(field.required_when, values))
	);
}

export function resolvedConnectorFieldDefault(
	field: StorageConnectorFieldDescriptor,
	values: FieldValues,
): StorageConnectorFieldValue | undefined {
	const rule = field.default_rules?.find((candidate) =>
		connectorFieldConditionsMatch(candidate.conditions, values),
	);
	return rule?.value ?? field.default_value ?? undefined;
}

export function connectorSelectOptions(
	field: StorageConnectorFieldDescriptor,
	values: FieldValues,
) {
	return (field.select?.options ?? []).filter((option) =>
		connectorFieldConditionsMatch(option.available_when, values),
	);
}

/**
 * 列出当前缺失的必填字段（可见、条件必填命中、无默认值兜底）。
 * 编辑模式传 allowSavedCredentials：secret/credential 字段留空表示沿用
 * 已保存凭证，不算缺失。
 */
export function missingRequiredConnectorFields(
	form: PolicyFormData,
	descriptor: StorageConnectorDescriptor | null | undefined,
	{ allowSavedCredentials = false }: { allowSavedCredentials?: boolean } = {},
): StorageConnectorFieldDescriptor[] {
	if (!descriptor) {
		return [];
	}
	const configValues = form.connector_config_values;
	return descriptor.fields.filter((field) => {
		if (field.scope === "action_input") {
			return false;
		}
		if (!isConnectorFieldVisible(field, configValues)) {
			return false;
		}
		if (!isConnectorFieldRequired(field, configValues)) {
			return false;
		}
		if (allowSavedCredentials && field.scope !== "connector_config") {
			return false;
		}
		const value =
			field.scope === "connector_config"
				? configValues[field.name]
				: form.credential_values[field.name];
		const resolved =
			value ?? resolvedConnectorFieldDefault(field, configValues);
		return resolved === undefined || resolved === null || resolved === "";
	});
}

export function normalizeConnectorConfigValues(
	values: Record<string, ConnectorFormValue>,
	descriptor: StorageConnectorDescriptor,
) {
	const fields = connectorConfigFields(descriptor);
	const working = declaredValues(values, fields);
	applyMissingDefaults(working, fields);
	for (let iteration = 0; iteration <= fields.length; iteration += 1) {
		if (!reconcileConditionalState(working, fields)) {
			break;
		}
	}
	return working;
}

export function applyConnectorConfigFieldTransition(
	values: Record<string, ConnectorFormValue>,
	descriptor: StorageConnectorDescriptor,
	fieldName: string,
	value: ConnectorFormValue | undefined,
	explicitDefaultFields: ReadonlySet<string> = new Set(),
) {
	const fields = connectorConfigFields(descriptor);
	const previous = { ...values };
	const previousEvaluation = declaredValues(values, fields);
	applyMissingDefaults(previousEvaluation, fields);

	const next = { ...previous };
	if (value === undefined) {
		delete next[fieldName];
	} else {
		next[fieldName] = value;
	}

	for (let iteration = 0; iteration <= fields.length; iteration += 1) {
		let changed = false;
		const nextEvaluation = declaredValues(next, fields);
		applyMissingDefaults(nextEvaluation, fields);
		for (const field of fields) {
			if (field.name === fieldName) {
				continue;
			}
			const previousDefault = resolvedConnectorFieldDefault(
				field,
				previousEvaluation,
			);
			const nextDefault = resolvedConnectorFieldDefault(field, nextEvaluation);
			const previousValue = previous[field.name];
			const tracksAutomaticDefault = Boolean(
				field.select?.automatic_default_label_key,
			);
			const previousValueWasAutomatic = tracksAutomaticDefault
				? !explicitDefaultFields.has(field.name) &&
					(previousValue === undefined || previousValue === previousDefault)
				: previousValue === undefined || previousValue === previousDefault;
			if (
				nextDefault !== previousDefault &&
				previousValueWasAutomatic &&
				next[field.name] !== nextDefault
			) {
				if (nextDefault === undefined) {
					delete next[field.name];
				} else {
					next[field.name] = nextDefault;
				}
				changed = true;
			}
		}
		changed = reconcileConditionalState(next, fields) || changed;
		if (!changed) {
			break;
		}
	}
	return next;
}

function connectorConfigFields(descriptor: StorageConnectorDescriptor) {
	return descriptor.fields.filter(
		(field) => field.scope === "connector_config",
	);
}

function declaredValues(
	values: Record<string, ConnectorFormValue>,
	fields: StorageConnectorFieldDescriptor[],
) {
	const declared = new Set(fields.map((field) => field.name));
	return Object.fromEntries(
		Object.entries(values).filter(([name]) => declared.has(name)),
	) as Record<string, ConnectorFormValue>;
}

function applyMissingDefaults(
	values: Record<string, ConnectorFormValue>,
	fields: StorageConnectorFieldDescriptor[],
) {
	const supplied = new Set(Object.keys(values));
	for (let iteration = 0; iteration <= fields.length; iteration += 1) {
		let changed = false;
		for (const field of fields) {
			if (supplied.has(field.name)) {
				continue;
			}
			const defaultValue = resolvedConnectorFieldDefault(field, values);
			if (defaultValue !== undefined && values[field.name] !== defaultValue) {
				values[field.name] = defaultValue;
				changed = true;
			} else if (defaultValue === undefined && field.name in values) {
				delete values[field.name];
				changed = true;
			}
		}
		if (!changed) {
			break;
		}
	}
}

function reconcileConditionalState(
	values: Record<string, ConnectorFormValue>,
	fields: StorageConnectorFieldDescriptor[],
) {
	let changed = false;
	for (const field of fields) {
		if (
			field.inactive_value_behavior === "clear" &&
			!isConnectorFieldVisible(field, values) &&
			field.name in values
		) {
			delete values[field.name];
			changed = true;
			continue;
		}

		if (
			field.kind === "select" &&
			(field.select?.options?.length ?? 0) > 0 &&
			field.select?.allow_custom_value !== true &&
			values[field.name] !== undefined &&
			!connectorSelectOptions(field, values).some(
				(option) => option.value === values[field.name],
			)
		) {
			const defaultValue = resolvedConnectorFieldDefault(field, values);
			if (defaultValue === undefined) {
				delete values[field.name];
			} else {
				values[field.name] = defaultValue;
			}
			changed = true;
		}
	}
	return changed;
}
