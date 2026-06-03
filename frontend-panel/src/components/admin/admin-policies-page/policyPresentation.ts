import type { DriverType } from "@/types/api";

export const PROTECTED_POLICY_ID = 1;

export function getPolicyDriverBadgeClass(driverType: DriverType): string {
	return driverType === "s3"
		? "border-blue-500/60 bg-blue-500/10 text-blue-600 dark:text-blue-300"
		: driverType === "tencent_cos"
			? "border-cyan-500/60 bg-cyan-500/10 text-cyan-700 dark:text-cyan-300"
			: driverType === "remote"
				? "border-amber-500/60 bg-amber-500/10 text-amber-600 dark:text-amber-300"
				: driverType === "google_drive"
					? "border-sky-500/60 bg-sky-500/10 text-sky-600 dark:text-sky-300"
					: "border-emerald-500/60 bg-emerald-500/10 text-emerald-600 dark:text-emerald-300";
}
