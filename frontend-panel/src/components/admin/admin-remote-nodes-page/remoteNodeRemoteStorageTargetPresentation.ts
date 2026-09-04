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
