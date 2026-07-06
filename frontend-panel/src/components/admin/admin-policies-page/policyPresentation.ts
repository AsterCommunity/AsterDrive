import type { DriverType } from "@/types/api";

export const PROTECTED_POLICY_ID = 1;

const POLICY_DRIVER_BADGE_CLASSES = {
	azure_blob: "border-sky-500/60 bg-sky-500/10 text-sky-700 dark:text-sky-300",
	local:
		"border-emerald-500/60 bg-emerald-500/10 text-emerald-600 dark:text-emerald-300",
	one_drive:
		"border-blue-500/60 bg-blue-500/10 text-blue-700 dark:text-blue-300",
	remote:
		"border-amber-500/60 bg-amber-500/10 text-amber-600 dark:text-amber-300",
	s3: "border-blue-500/60 bg-blue-500/10 text-blue-600 dark:text-blue-300",
	sftp: "border-violet-500/60 bg-violet-500/10 text-violet-700 dark:text-violet-300",
	tencent_cos:
		"border-cyan-500/60 bg-cyan-500/10 text-cyan-700 dark:text-cyan-300",
} satisfies Record<DriverType, string>;

export function getPolicyDriverBadgeClass(driverType: DriverType): string {
	return POLICY_DRIVER_BADGE_CLASSES[driverType];
}
