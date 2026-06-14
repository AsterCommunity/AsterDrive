import type { DriverType } from "@/types/api";

export const PROTECTED_POLICY_ID = 1;

function assertNever(value: never): never {
	throw new Error(`Unhandled storage policy driver type: ${value}`);
}

export function getPolicyDriverBadgeClass(driverType: DriverType): string {
	return driverType === "s3"
		? "border-blue-500/60 bg-blue-500/10 text-blue-600 dark:text-blue-300"
		: driverType === "tencent_cos"
			? "border-cyan-500/60 bg-cyan-500/10 text-cyan-700 dark:text-cyan-300"
			: driverType === "azure_blob"
				? "border-sky-500/60 bg-sky-500/10 text-sky-700 dark:text-sky-300"
				: driverType === "remote"
					? "border-amber-500/60 bg-amber-500/10 text-amber-600 dark:text-amber-300"
					: "border-emerald-500/60 bg-emerald-500/10 text-emerald-600 dark:text-emerald-300";
}

export function getPolicyDriverLabelKey(driverType: DriverType): string {
	switch (driverType) {
		case "local":
			return "driver_type_local";
		case "remote":
			return "driver_type_remote";
		case "tencent_cos":
			return "driver_type_tencent_cos";
		case "azure_blob":
			return "driver_type_azure_blob";
		case "s3":
			return "driver_type_s3";
		default:
			return assertNever(driverType);
	}
}
