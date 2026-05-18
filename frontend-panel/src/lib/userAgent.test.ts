import { describe, expect, it } from "vitest";
import {
	formatPasskeyDefaultName,
	formatUserAgentLabel,
	parseUserAgent,
} from "@/lib/userAgent";

describe("userAgent helpers", () => {
	it("formats a desktop edge user agent into a compact label", () => {
		const userAgent =
			"Mozilla/5.0 (Macintosh; Intel Mac OS X 10_15_7) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/147.0.0.0 Safari/537.36 Edg/147.0.0.0";

		expect(parseUserAgent(userAgent)).toEqual({
			browserFamily: "edge",
			browserName: "Edge",
			browserVersion: "147",
			deviceType: "desktop",
			osFamily: "macos",
			osName: "macOS 10.15.7",
		});
		expect(formatUserAgentLabel(userAgent)).toBe(
			"Edge 147 · macOS 10.15.7 · Desktop",
		);
	});

	it("formats a mobile safari user agent", () => {
		const userAgent =
			"Mozilla/5.0 (iPhone; CPU iPhone OS 18_3 like Mac OS X) AppleWebKit/605.1.15 (KHTML, like Gecko) Version/18.3 Mobile/15E148 Safari/604.1";

		expect(formatUserAgentLabel(userAgent)).toBe(
			"Safari 18.3 · iOS 18.3 · Mobile",
		);
	});

	it("treats modern ipad safari user agents as ipados tablets", () => {
		const userAgent =
			"Mozilla/5.0 (Macintosh; Intel Mac OS X 10_15_7) AppleWebKit/605.1.15 (KHTML, like Gecko) Version/18.3 Mobile/15E148 Safari/604.1";

		expect(parseUserAgent(userAgent)).toEqual({
			browserFamily: "safari",
			browserName: "Safari",
			browserVersion: "18.3",
			deviceType: "tablet",
			osFamily: "ipados",
			osName: "iPadOS",
		});
		expect(formatUserAgentLabel(userAgent)).toBe(
			"Safari 18.3 · iPadOS · Tablet",
		);
	});

	it("detects android tablets without a Mobile marker", () => {
		const userAgent =
			"Mozilla/5.0 (Linux; Android 14; Pixel Tablet) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/146.0.0.0 Safari/537.36";

		expect(formatUserAgentLabel(userAgent)).toBe(
			"Chrome 146 · Android 14 · Tablet",
		);
	});

	it("keeps unknown custom agents intact instead of hiding useful details", () => {
		expect(parseUserAgent("Vitest Browser/1.0")).toEqual({
			browserFamily: "unknown",
			browserName: null,
			browserVersion: null,
			deviceType: "unknown",
			osFamily: "unknown",
			osName: null,
		});
		expect(formatUserAgentLabel("Vitest Browser/1.0")).toBe(
			"Vitest Browser/1.0",
		);
	});

	it("falls back to the provided unknown label when the user agent is empty", () => {
		expect(
			formatUserAgentLabel("", {
				unknown: "未知设备",
			}),
		).toBe("未知设备");
	});

	it("formats the passkey default name as operating system and browser", () => {
		const userAgent =
			"Mozilla/5.0 (Macintosh; Intel Mac OS X 10_15_7) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/147.0.0.0 Safari/537.36 Edg/147.0.0.0";

		expect(formatPasskeyDefaultName(userAgent)).toBe("macOS - Edge");
	});

	it("normalizes passkey default names across known operating systems", () => {
		expect(
			formatPasskeyDefaultName(
				"Mozilla/5.0 (Linux; Android 14; Pixel 8) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/146.0.0.0 Mobile Safari/537.36",
			),
		).toBe("Android - Chrome");
		expect(
			formatPasskeyDefaultName(
				"Mozilla/5.0 (X11; CrOS x86_64 15886.44.0) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/146.0.0.0 Safari/537.36",
			),
		).toBe("ChromeOS - Chrome");
		expect(
			formatPasskeyDefaultName(
				"Mozilla/5.0 (iPhone; CPU iPhone OS 18_3 like Mac OS X) AppleWebKit/605.1.15 (KHTML, like Gecko) Version/18.3 Mobile/15E148 Safari/604.1",
			),
		).toBe("iOS - Safari");
		expect(
			formatPasskeyDefaultName(
				"Mozilla/5.0 (Macintosh; Intel Mac OS X 10_15_7) AppleWebKit/605.1.15 (KHTML, like Gecko) Version/18.3 Mobile/15E148 Safari/604.1",
			),
		).toBe("iPadOS - Safari");
		expect(
			formatPasskeyDefaultName(
				"Mozilla/5.0 (X11; Linux x86_64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/146.0.0.0 Safari/537.36",
			),
		).toBe("Linux - Chrome");
	});

	it("falls back when passkey default name cannot identify the client", () => {
		expect(formatPasskeyDefaultName("Vitest Browser/1.0", "Passkey")).toBe(
			"Passkey",
		);
		expect(
			formatPasskeyDefaultName("CustomAgent/1.0 (Windows)", "Passkey"),
		).toBe("Passkey");
		expect(formatPasskeyDefaultName("", "Passkey")).toBe("Passkey");
	});
});
