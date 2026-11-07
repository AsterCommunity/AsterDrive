import { createServer } from "node:http";
import { authenticate, logout } from "./support/auth";
import { ADMIN } from "./support/fixtures";
import { apiJsonInPage } from "./support/network";
import { expect, test } from "./support/test";

test("external recovery completes in the initiating browser after the provider redirect", async ({
	page,
	request,
}) => {
	const provider = createServer((req, res) => {
		const url = new URL(req.url ?? "/", "http://localhost");
		if (url.pathname === "/authorize") {
			const callback = new URL(
				url.searchParams.get("redirect_uri") ?? "http://invalid",
			);
			callback.searchParams.set("state", url.searchParams.get("state") ?? "");
			callback.searchParams.set("code", "local-provider-code");
			res.writeHead(302, { Location: callback.toString() });
			res.end();
			return;
		}
		res.setHeader("Content-Type", "application/json");
		res.end(
			JSON.stringify(
				url.pathname === "/token"
					? {
							access_token: "local-fixture-provider-token",
							token_type: "Bearer",
							expires_in: 60,
						}
					: {
							id: "browser-bound-subject",
							login: "local-profile",
							name: "Local provider",
						},
			),
		);
	});
	await new Promise<void>((resolve) =>
		provider.listen(0, "127.0.0.1", resolve),
	);
	const address = provider.address();
	if (!address || typeof address === "string")
		throw new Error("provider did not bind");
	const endpoint = `http://127.0.0.1:${address.port}`;
	try {
		await authenticate(page, request);
		const stored = await apiJsonInPage<{ key: string }>(
			page,
			"/api/v1/admin/external-auth/providers",
			{
				method: "POST",
				withCsrf: true,
				body: {
					provider_kind: "generic_oauth2",
					callback_mode: "legacy",
					display_name: "Recovery fixture",
					authorization_url: `${endpoint}/authorize`,
					token_url: `${endpoint}/token`,
					userinfo_url: `${endpoint}/userinfo`,
					client_id: "fixture",
					client_secret: "fixture-local-secret",
					scopes: "profile",
					enabled: true,
					auto_provision_enabled: false,
					auto_link_verified_email_enabled: false,
					require_email_verified: false,
					subject_claim: "id",
					username_claim: "login",
					display_name_claim: "name",
					email_claim: "email",
					email_verified_claim: "email_verified",
					allowed_domains: [],
				},
			},
		);
		await logout(page);
		const start = await apiJsonInPage<{ authorization_url: string }>(
			page,
			`/api/v1/auth/external-auth/generic_oauth2/${stored.key}/start`,
			{ method: "POST", body: { return_path: "/" } },
		);
		await page.goto(start.authorization_url);
		await expect(page).toHaveURL(/external_auth=email_required/);
		const flow = new URL(page.url()).searchParams.get("flow");
		expect(flow).toBeTruthy();
		const binding = (await page.context().cookies()).find((cookie) =>
			cookie.name.startsWith("aster_external_recovery_"),
		);
		expect(binding?.httpOnly).toBe(true);
		expect(binding?.sameSite).toBe("Lax");
		await page
			.locator("#external-auth-password-link-identifier")
			.fill(ADMIN.email);
		await page
			.locator("#external-auth-password-link-password")
			.fill(ADMIN.password);
		await page
			.getByRole("button", { name: "Sign in and link", exact: true })
			.click();
		await expect(page).toHaveURL(/\/$/);
		const links = await apiJsonInPage<Array<{ subject: string }>>(
			page,
			"/api/v1/auth/external-auth/links",
		);
		expect(links.some((link) => link.subject === "browser-bound-subject")).toBe(
			true,
		);
	} finally {
		await new Promise<void>((resolve, reject) =>
			provider.close((error) => (error ? reject(error) : resolve())),
		);
	}
});
