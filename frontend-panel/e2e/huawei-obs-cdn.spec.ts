import { authenticate, gotoAdminPage } from "./support/auth";
import { uniqueName } from "./support/fixtures";
import { expect, test } from "./support/test";

test("configures Huawei CDN Type A SHA-256 and preserves credentials across edits and mode changes", async ({
	page,
	request,
}) => {
	await authenticate(page, request);
	// Provider HTTP contracts are covered by the driver tests. This browser
	// test exercises the real catalog, form, persistence and edit behavior.
	await page.route("**/api/v1/admin/policies/test", async (route) => {
		await route.fulfill({
			status: 200,
			json: { code: "success", msg: "success", data: {} },
		});
	});
	await gotoAdminPage(page, "/admin/policies", "Storage Policies");
	await page.getByRole("button", { name: "New Policy" }).click();
	await page
		.getByRole("button", { name: "Huawei Cloud OBS", exact: true })
		.click();
	await page.getByRole("button", { name: "Next", exact: true }).click();
	await page.locator("#name").fill(uniqueName("obs-private-cdn"));
	await page
		.locator("#endpoint")
		.fill("https://obs.cn-north-4.myhuaweicloud.com");
	await page.locator("#bucket").fill("archive-bucket");
	await page.locator("#obs_region").fill("cn-north-4");
	await page
		.getByLabel("Huawei Cloud Access Key ID", { exact: true })
		.fill("playwright-access-key");
	await page
		.getByLabel("Huawei Cloud Secret Access Key", { exact: true })
		.fill("playwright-origin-secret");
	const delivery = page.locator("#download_delivery");
	const cdnKey = page.getByLabel("CDN authentication key", { exact: true });
	await expect(page.locator("#download_base_url")).toHaveCount(0);
	await expect(cdnKey).toHaveCount(0);
	await delivery.click();
	await page
		.getByRole("option", {
			name: "Huawei CDN accelerated download (Type A + SHA-256)",
			exact: true,
		})
		.click();
	await expect(page.locator("#download_base_url")).toBeVisible();
	await expect(cdnKey).toBeVisible();
	await expect(page.locator("#obs_cdn_auth_validity_seconds")).toHaveValue(
		"300",
	);
	await expect(page.locator("#obs_cdn_auth_param")).toHaveValue("auth_key");
	await page.locator("#download_base_url").fill("https://cdn.example.test");
	await page.locator("#obs_cdn_auth_param").fill("Download_Auth");
	await page.locator("#object_storage_download_strategy").click();
	await page
		.getByRole("option", { name: "Presigned Redirect", exact: true })
		.click();
	await page.getByRole("button", { name: "Review", exact: true }).click();
	await expect(cdnKey).toBeVisible();
	await cdnKey.fill("PlaywrightCdnSecret012345");
	await page.getByRole("button", { name: "Review", exact: true }).click();
	const [created] = await Promise.all([
		page.waitForResponse(
			(response) =>
				response.request().method() === "POST" &&
				response.url().endsWith("/api/v1/admin/policies"),
		),
		page.getByRole("button", { name: "Create", exact: true }).click(),
	]);
	expect(created.status()).toBe(201);
	expect(
		created.request().postDataJSON().connection.connector_config.values,
	).toMatchObject({
		download_delivery: "huawei_cdn_type_a_sha256",
		download_base_url: "https://cdn.example.test",
		obs_cdn_auth_validity_seconds: 300,
		obs_cdn_auth_param: "Download_Auth",
	});
	expect(await created.text()).not.toContain("PlaywrightCdnSecret012345");
	await expect(page).toHaveURL(/\/admin\/policies\/\d+$/);
	await expect(cdnKey).toHaveValue("");
	await page.locator("#obs_cdn_auth_validity_seconds").fill("120");
	const [updated] = await Promise.all([
		page.waitForResponse(
			(response) =>
				response.request().method() === "PATCH" &&
				response.url().includes("/api/v1/admin/policies/"),
		),
		page.getByRole("button", { name: "Save Changes" }).first().click(),
	]);
	expect(updated.ok()).toBe(true);
	await page.reload();
	await expect(page.locator("#obs_cdn_auth_validity_seconds")).toHaveValue(
		"120",
	);
	await expect(page.locator("#obs_cdn_auth_param")).toHaveValue(
		"Download_Auth",
	);
	await expect(cdnKey).toHaveValue("");
	await delivery.click();
	await page
		.getByRole("option", {
			name: "OBS origin (native OBS signature)",
			exact: true,
		})
		.click();
	await expect(page.locator("#download_base_url")).toHaveCount(0);
	await expect(cdnKey).toHaveCount(0);
	const [origin] = await Promise.all([
		page.waitForResponse(
			(response) =>
				response.request().method() === "PATCH" &&
				response.url().includes("/api/v1/admin/policies/"),
		),
		page.getByRole("button", { name: "Save Changes" }).first().click(),
	]);
	expect(origin.ok()).toBe(true);
	await page.reload();
	await delivery.click();
	await page
		.getByRole("option", {
			name: "Huawei CDN accelerated download (Type A + SHA-256)",
			exact: true,
		})
		.click();
	await expect(page.locator("#download_base_url")).toHaveValue(
		"https://cdn.example.test",
	);
	await expect(page.locator("#obs_cdn_auth_validity_seconds")).toHaveValue(
		"120",
	);
	await expect(page.locator("#obs_cdn_auth_param")).toHaveValue(
		"Download_Auth",
	);
	const [restored] = await Promise.all([
		page.waitForResponse(
			(response) =>
				response.request().method() === "PATCH" &&
				response.url().includes("/api/v1/admin/policies/"),
		),
		page.getByRole("button", { name: "Save Changes" }).first().click(),
	]);
	expect(restored.ok()).toBe(true);
	expect(await restored.text()).not.toContain("PlaywrightCdnSecret012345");
});
