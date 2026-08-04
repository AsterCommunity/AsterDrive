import { describe, expect, it } from "vitest";
import {
	getStorageConnectorBadgePresentation,
	PROTECTED_POLICY_ID,
} from "./policyPresentation";

describe("policyPresentation", () => {
	it("renders a connector-owned RGB color without connector id branches", () => {
		expect(PROTECTED_POLICY_ID).toBe(1);
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
