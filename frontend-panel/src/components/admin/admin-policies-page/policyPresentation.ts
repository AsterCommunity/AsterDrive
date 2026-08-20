import type { CSSProperties } from "react";
import type { StorageConnectorBadgeRgb } from "@/types/api";

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
