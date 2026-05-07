import { adminConfigService } from "@/services/adminService";
import type { ConfigSchemaItem, TemplateVariableGroup } from "@/types/api";

let cachedSchema: ConfigSchemaItem[] | null = null;
let cachedTemplateVariables: TemplateVariableGroup[] | null = null;
let pendingSchemaRequest: Promise<ConfigSchemaItem[]> | null = null;
let pendingTemplateVariablesRequest: Promise<TemplateVariableGroup[]> | null =
	null;

export function readAdminConfigSchemaCache() {
	return cachedSchema;
}

export function readAdminTemplateVariablesCache() {
	return cachedTemplateVariables;
}

export function invalidateAdminConfigMetadataCache() {
	cachedSchema = null;
	cachedTemplateVariables = null;
	pendingSchemaRequest = null;
	pendingTemplateVariablesRequest = null;
}

export async function loadAdminConfigSchema(options?: { force?: boolean }) {
	const force = options?.force ?? false;
	if (!force && cachedSchema != null) {
		return cachedSchema;
	}
	if (!force && pendingSchemaRequest != null) {
		return pendingSchemaRequest;
	}

	pendingSchemaRequest = adminConfigService
		.schema()
		.then((schema) => {
			cachedSchema = schema;
			return schema;
		})
		.finally(() => {
			pendingSchemaRequest = null;
		});

	return pendingSchemaRequest;
}

export async function loadAdminTemplateVariables(options?: {
	force?: boolean;
}) {
	const force = options?.force ?? false;
	if (!force && cachedTemplateVariables != null) {
		return cachedTemplateVariables;
	}
	if (!force && pendingTemplateVariablesRequest != null) {
		return pendingTemplateVariablesRequest;
	}

	pendingTemplateVariablesRequest = adminConfigService
		.templateVariables()
		.then((templateVariables) => {
			cachedTemplateVariables = templateVariables;
			return templateVariables;
		})
		.finally(() => {
			pendingTemplateVariablesRequest = null;
		});

	return pendingTemplateVariablesRequest;
}
