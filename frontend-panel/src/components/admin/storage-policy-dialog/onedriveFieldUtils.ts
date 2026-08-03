import { connectorStringValue } from "./formTypes";
import type { SharedFieldProps } from "./StoragePolicyFieldTypes";

export const MICROSOFT_GRAPH_PROVIDER = "microsoft_graph";

export const ONE_DRIVE_CUSTOM_TENANT_MODE = "custom";
export const ONE_DRIVE_AUTO_TENANT_MODE = "auto";

export type OneDriveTenantMode =
	| typeof ONE_DRIVE_AUTO_TENANT_MODE
	| "consumers"
	| "organizations"
	| "common"
	| typeof ONE_DRIVE_CUSTOM_TENANT_MODE;

export function getDefaultTenant(mode: string) {
	if (mode === "personal") {
		return "consumers";
	}
	if (mode === "work_or_school") {
		return "common";
	}
	return "organizations";
}

export function getTenantMode(
	form: SharedFieldProps["form"],
): OneDriveTenantMode {
	const tenant = connectorStringValue(form, "tenant").trim();
	const accountMode = connectorStringValue(
		form,
		"account_mode",
		"work_or_school",
	);
	if (!tenant || tenant === getDefaultTenant(accountMode)) {
		return ONE_DRIVE_AUTO_TENANT_MODE;
	}
	if (
		tenant === "consumers" ||
		tenant === "organizations" ||
		tenant === "common"
	) {
		return tenant;
	}
	return ONE_DRIVE_CUSTOM_TENANT_MODE;
}
