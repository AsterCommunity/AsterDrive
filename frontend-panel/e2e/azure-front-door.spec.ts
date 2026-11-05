import { authenticate, gotoAdminPage } from "./support/auth";
import { uniqueName } from "./support/fixtures";
import { expect, test } from "./support/test";

test("configures Azure Front Door delivery and preserves cache settings across edits and mode changes", async ({
	page,
	request,
}) => {
	await authenticate(page, request);
	// Exercise the real catalog, form and persistence; provider requests are
	// covered by the Rust capture, signature and Azurite tests.
	await page.route("**/api/v1/admin/policies/test", async (route) => {
		await route.fulfill({
			status: 200,
			json: { code: "success", msg: "success", data: {} },
		});
	});
	await gotoAdminPage(page, "/admin/policies", "Storage Policies");
	await page.getByRole("button", { name: "New Policy" }).click();
	await page.getByRole("button", { name: "Azure Blob", exact: true }).click();
	await page.getByRole("button", { name: "Next", exact: true }).click();
	await page.locator("#name").fill(uniqueName("azure-front-door"));
	await page.locator("#endpoint").fill("https://account.blob.core.windows.net");
	await page.locator("#bucket").fill("archive");
	await page
		.getByLabel("Storage Account Name", { exact: true })
		.fill("account");
	await page
		.getByLabel("Storage Account Key", { exact: true })
		.fill("c2VjcmV0");
	const delivery = page.locator("#download_delivery");
	const cache = page.locator("#azure_cdn_cache_max_age_seconds");
	await expect(page.locator("#download_base_url")).toHaveCount(0);
	await expect(cache).toHaveCount(0);
	await delivery.click();
	await page
		.getByRole("option", { name: "Azure Front Door", exact: true })
		.click();
	await expect(page.locator("#download_base_url")).toBeVisible();
	await expect(cache).toHaveValue("0");
	await expect(cache).toHaveAttribute("min", "0");
	await expect(cache).toHaveAttribute("max", "31622400");
	await page.locator("#download_base_url").fill("https://files.example.test");
	await cache.fill("300");
	await page.locator("#object_storage_download_strategy").click();
	await page
		.getByRole("option", { name: "Presigned Redirect", exact: true })
		.click();
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
		endpoint: "https://account.blob.core.windows.net",
		download_delivery: "azure_front_door_sas",
		download_base_url: "https://files.example.test",
		azure_cdn_cache_max_age_seconds: 300,
		object_storage_download_strategy: "presigned",
	});
	await expect(page).toHaveURL(/\/admin\/policies\/\d+$/);
	await cache.fill("60");
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
	await expect(cache).toHaveValue("60");
	await expect(
		page.getByLabel("Storage Account Key", { exact: true }),
	).toHaveValue("");
	await delivery.click();
	await page
		.getByRole("option", { name: "Azure Blob origin", exact: true })
		.click();
	await expect(cache).toHaveCount(0);
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
		.getByRole("option", { name: "Azure Front Door", exact: true })
		.click();
	await expect(page.locator("#download_base_url")).toHaveValue(
		"https://files.example.test",
	);
	await expect(cache).toHaveValue("60");
});
