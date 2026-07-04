import type { RemoteStorageTargetInfo } from "@/types/api";

export function getRemoteNodeRemoteStorageTargetProfileStatus(
	profile: RemoteStorageTargetInfo,
) {
	if (profile.last_error.trim()) {
		return {
			labelKey: "remote_node_ingress_profile_status_error",
			toneClass:
				"border-destructive/50 bg-destructive/10 text-destructive dark:border-destructive/40",
		};
	}

	if (profile.applied_revision < profile.desired_revision) {
		return {
			labelKey: "remote_node_ingress_profile_status_pending",
			toneClass:
				"border-amber-500/60 bg-amber-500/10 text-amber-700 dark:text-amber-300",
		};
	}

	return {
		labelKey: "remote_node_ingress_profile_status_ready",
		toneClass:
			"border-emerald-500/60 bg-emerald-500/10 text-emerald-700 dark:text-emerald-300",
	};
}

export function getRemoteNodeRemoteStorageTargetDriverBadgeTone(
	driverType: RemoteStorageTargetInfo["driver_type"],
) {
	return driverType === "s3"
		? "border-blue-500/60 bg-blue-500/10 text-blue-700 dark:text-blue-300"
		: "border-slate-500/50 bg-slate-500/10 text-slate-700 dark:text-slate-300";
}
