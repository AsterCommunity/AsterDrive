import type { CSSProperties } from "react";
import type { IconName } from "@/components/ui/icon";
import type {
	StorageConnectorBadgeRgb,
	StoragePolicyRecoveryProbe,
} from "@/types/api";

const DEFAULT_BADGE_RGB: StorageConnectorBadgeRgb = {
	red: 113,
	green: 113,
	blue: 122,
};

type BadgeStyle = CSSProperties & {
	"--storage-connector-badge-border": string;
	"--storage-connector-badge-background": string;
	"--storage-connector-badge-foreground": string;
	"--storage-connector-badge-foreground-dark": string;
};

function channel(value: number) {
	return Math.min(255, Math.max(0, Math.round(value)));
}

function rgbString(rgb: StorageConnectorBadgeRgb) {
	return `${channel(rgb.red)} ${channel(rgb.green)} ${channel(rgb.blue)}`;
}

function mixWith(
	rgb: StorageConnectorBadgeRgb,
	target: number,
	amount: number,
) {
	return `${channel(rgb.red * (1 - amount) + target * amount)} ${channel(
		rgb.green * (1 - amount) + target * amount,
	)} ${channel(rgb.blue * (1 - amount) + target * amount)}`;
}

export function getStorageConnectorBadgePresentation(
	rgb: StorageConnectorBadgeRgb | null | undefined,
) {
	const color = rgb ?? DEFAULT_BADGE_RGB;
	const style: BadgeStyle = {
		"--storage-connector-badge-border": `rgb(${rgbString(color)} / 0.55)`,
		"--storage-connector-badge-background": `rgb(${rgbString(color)} / 0.12)`,
		"--storage-connector-badge-foreground": `rgb(${mixWith(color, 0, 0.35)})`,
		"--storage-connector-badge-foreground-dark": `rgb(${mixWith(color, 255, 0.35)})`,
	};
	return {
		className:
			"border-[var(--storage-connector-badge-border)] bg-[var(--storage-connector-badge-background)] text-[var(--storage-connector-badge-foreground)] dark:text-[var(--storage-connector-badge-foreground-dark)]",
		style,
	};
}

type RecoveryStatus = StoragePolicyRecoveryProbe["status"];
type RecoveryPresentationKey =
	| "policy_recovery_probe_running"
	| "policy_recovery_probe_recoverable_title"
	| "policy_recovery_probe_partial_title"
	| "policy_recovery_probe_blocked_title"
	| "policy_recovery_probe_indeterminate_title"
	| "policy_recovery_probe_virtual_empty_title";

interface StoragePolicyRecoveryStatusPresentation {
	icon: IconName;
	titleKey: RecoveryPresentationKey;
	toneClass: string;
}

const RECOVERY_TONE_CLASSES = {
	neutral: "border-border bg-muted/20 text-foreground",
	success:
		"border-emerald-300 bg-emerald-50 text-emerald-950 dark:border-emerald-900 dark:bg-emerald-950/35 dark:text-emerald-100",
	warning:
		"border-amber-300 bg-amber-50 text-amber-950 dark:border-amber-900 dark:bg-amber-950/35 dark:text-amber-100",
	danger: "border-destructive/40 bg-destructive/8 text-destructive",
} as const;

const RECOVERY_STATUS_PRESENTATIONS = {
	recoverable: {
		icon: "Check",
		titleKey: "policy_recovery_probe_recoverable_title",
		toneClass: RECOVERY_TONE_CLASSES.success,
	},
	partially_recoverable: {
		icon: "Warning",
		titleKey: "policy_recovery_probe_partial_title",
		toneClass: RECOVERY_TONE_CLASSES.warning,
	},
	blocked: {
		icon: "CircleAlert",
		titleKey: "policy_recovery_probe_blocked_title",
		toneClass: RECOVERY_TONE_CLASSES.danger,
	},
	indeterminate: {
		icon: "Question",
		titleKey: "policy_recovery_probe_indeterminate_title",
		toneClass: RECOVERY_TONE_CLASSES.warning,
	},
	no_stored_objects: {
		icon: "Cloud",
		titleKey: "policy_recovery_probe_virtual_empty_title",
		toneClass: RECOVERY_TONE_CLASSES.neutral,
	},
} as const satisfies Record<
	RecoveryStatus,
	StoragePolicyRecoveryStatusPresentation
>;

/** Maps recovery state to one shared semantic icon, title, and color presentation. */
export function getStoragePolicyRecoveryStatusPresentation(
	status: RecoveryStatus | null | undefined,
	loading: boolean,
): StoragePolicyRecoveryStatusPresentation {
	if (loading || !status) {
		return {
			icon: loading ? "Spinner" : "Cloud",
			titleKey: loading
				? "policy_recovery_probe_running"
				: "policy_recovery_probe_virtual_empty_title",
			toneClass: RECOVERY_TONE_CLASSES.neutral,
		};
	}
	return RECOVERY_STATUS_PRESENTATIONS[status];
}
