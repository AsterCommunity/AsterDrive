import type { APIRequestContext, Page } from "@playwright/test";
import { authenticate } from "./support/auth";
import { uniqueName } from "./support/fixtures";
import { apiJsonInPage } from "./support/network";
import { expect, test } from "./support/test";

type PublicCustomConfig = {
	entries: Record<string, string>;
};

async function setCustomConfig(
	page: Page,
	key: string,
	value: string,
	visibility: "authenticated" | "private" | "public",
) {
	return apiJsonInPage(page, `/api/v1/admin/config/${key}`, {
		body: {
			value,
			visibility,
		},
		method: "PUT",
		withCsrf: true,
	});
}

async function fetchPublicCustomConfigInPage(page: Page) {
	const result = await page.evaluate(async () => {
		const response = await fetch("/api/v1/public/custom-config", {
			credentials: "include",
		});
		return {
			cacheControl: response.headers.get("Cache-Control"),
			status: response.status,
			text: await response.text(),
		};
	});

	expect(result.status).toBe(200);
	const payload = JSON.parse(result.text) as {
		code: number;
		data: PublicCustomConfig;
	};
	expect(payload.code).toBe(0);
	return {
		cacheControl: result.cacheControl,
		config: payload.data,
	};
}

async function fetchPublicCustomConfigWithRequest(request: APIRequestContext) {
	const response = await request.get("/api/v1/public/custom-config");
	expect(response.status()).toBe(200);
	const payload = (await response.json()) as {
		code: number;
		data: PublicCustomConfig;
	};
	expect(payload.code).toBe(0);
	expect(response.headers()["cache-control"]).toBe("public, max-age=60");
	return payload.data;
}

test.describe
	.serial("Public custom config E2E", () => {
		test("exposes only custom entries allowed for the current request identity", async ({
			page,
			request,
		}) => {
			await authenticate(page, request);

			const suffix = uniqueName("custom-config").replaceAll("-", "_");
			const publicKey = `e2e.${suffix}_public`;
			const authenticatedKey = `e2e.${suffix}_authenticated`;
			const privateKey = `e2e.${suffix}_private`;

			await setCustomConfig(page, publicKey, "public-value", "public");
			await setCustomConfig(
				page,
				authenticatedKey,
				"authenticated-value",
				"authenticated",
			);
			await setCustomConfig(page, privateKey, "private-value", "private");

			const authenticated = await fetchPublicCustomConfigInPage(page);
			expect(authenticated.cacheControl).toBe("private, max-age=60");
			expect(authenticated.config.entries[publicKey]).toBe("public-value");
			expect(authenticated.config.entries[authenticatedKey]).toBe(
				"authenticated-value",
			);
			expect(authenticated.config.entries[privateKey]).toBeUndefined();

			const anonymous = await fetchPublicCustomConfigWithRequest(request);
			expect(anonymous.entries[publicKey]).toBe("public-value");
			expect(anonymous.entries[authenticatedKey]).toBeUndefined();
			expect(anonymous.entries[privateKey]).toBeUndefined();

			const invalidTokenResult = await request.get(
				"/api/v1/public/custom-config",
				{
					headers: {
						Authorization: "Bearer invalid.token.value",
					},
				},
			);
			expect(invalidTokenResult.status()).toBe(401);
		});
	});
