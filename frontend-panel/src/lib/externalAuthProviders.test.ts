import { describe, expect, it } from "vitest";
import {
	externalAuthKindIconPath,
	normalizeExternalAuthIconUrl,
} from "@/lib/externalAuthProviders";

describe("external auth provider helpers", () => {
	it("returns the bundled icon for known provider kinds", () => {
		expect(externalAuthKindIconPath("oidc")).toBe(
			"/static/external-auth/openid-seeklogo.svg",
		);
		expect(externalAuthKindIconPath("generic_oauth2")).toBe(
			"/static/external-auth/oauth_logo.svg",
		);
		expect(externalAuthKindIconPath("unknown" as never)).toBe("");
	});

	it("normalizes only safe relative paths and http URLs", () => {
		expect(normalizeExternalAuthIconUrl(null)).toBe("");
		expect(normalizeExternalAuthIconUrl(undefined)).toBe("");
		expect(normalizeExternalAuthIconUrl("  ")).toBe("");
		expect(normalizeExternalAuthIconUrl(" /static/idp.svg ")).toBe(
			"/static/idp.svg",
		);
		expect(normalizeExternalAuthIconUrl("//cdn.example.com/idp.svg")).toBe("");
		expect(normalizeExternalAuthIconUrl("/static/idp icon.svg")).toBe("");
		expect(
			normalizeExternalAuthIconUrl("https://idp.example.com/icon.svg"),
		).toBe("https://idp.example.com/icon.svg");
		expect(
			normalizeExternalAuthIconUrl("http://idp.example.com/icon.svg"),
		).toBe("http://idp.example.com/icon.svg");
		expect(normalizeExternalAuthIconUrl("ftp://idp.example.com/icon.svg")).toBe(
			"",
		);
		expect(normalizeExternalAuthIconUrl("not a url")).toBe("");
	});
});
