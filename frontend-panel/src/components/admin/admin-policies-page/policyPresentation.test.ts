import { describe, expect, it } from "vitest";
import {
	getStorageConnectorBadgePresentation,
	getStoragePolicyRecoveryStatusPresentation,
} from "./policyPresentation";

describe("policyPresentation", () => {
	it("renders a connector-owned RGB color without connector id branches", () => {
		const presentation = getStorageConnectorBadgePresentation({
			red: 16,
			green: 185,
			blue: 129,
		});

		expect(presentation.className).toContain(
			"--storage-connector-badge-background",
		);
		expect(presentation.style).toMatchObject({
			"--storage-connector-badge-background": "rgb(16 185 129 / 0.12)",
			"--storage-connector-badge-border": "rgb(16 185 129 / 0.55)",
			"--storage-connector-badge-foreground": "rgb(10 120 84)",
			"--storage-connector-badge-foreground-dark": "rgb(100 210 173)",
		});
	});

	it("uses a neutral fallback and clamps malformed runtime channels", () => {
		expect(
			getStorageConnectorBadgePresentation(undefined).style[
				"--storage-connector-badge-border"
			],
		).toBe("rgb(113 113 122 / 0.55)");

		const presentation = getStorageConnectorBadgePresentation({
			red: -20,
			green: 400,
			blue: 12.6,
		});
		expect(presentation.style["--storage-connector-badge-border"]).toBe(
			"rgb(0 255 13 / 0.55)",
		);
	});
});

describe("storage policy recovery status presentation", () => {
	it("maps success, warning, danger, and neutral states centrally", () => {
		expect(
			getStoragePolicyRecoveryStatusPresentation("recoverable", false),
		).toMatchObject({
			icon: "Check",
			toneClass: expect.stringContaining("emerald"),
		});
		expect(
			getStoragePolicyRecoveryStatusPresentation(
				"partially_recoverable",
				false,
			),
		).toMatchObject({
			icon: "Warning",
			toneClass: expect.stringContaining("amber"),
		});
		expect(
			getStoragePolicyRecoveryStatusPresentation("blocked", false),
		).toMatchObject({
			icon: "CircleAlert",
			toneClass: expect.stringContaining("destructive"),
		});
		expect(
			getStoragePolicyRecoveryStatusPresentation("indeterminate", false),
		).toMatchObject({
			icon: "Question",
			toneClass: expect.stringContaining("amber"),
		});
		expect(
			getStoragePolicyRecoveryStatusPresentation("no_stored_objects", false),
		).toMatchObject({
			icon: "Cloud",
			toneClass: expect.stringContaining("muted"),
		});
	});

	it("keeps an unrequested probe distinct from a policy with no stored objects", () => {
		expect(
			getStoragePolicyRecoveryStatusPresentation(undefined, false),
		).toMatchObject({
			icon: "Cloud",
			titleKey: "policy_recovery_probe_pending",
			toneClass: expect.stringContaining("muted"),
		});
	});

	it("uses a neutral spinner presentation while a probe is running", () => {
		expect(
			getStoragePolicyRecoveryStatusPresentation("blocked", true),
		).toMatchObject({
			icon: "Spinner",
			toneClass: expect.stringContaining("muted"),
		});
	});
});
