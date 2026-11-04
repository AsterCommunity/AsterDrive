import { authenticate, gotoAdminPage } from "./support/auth";
import { uniqueName } from "./support/fixtures";
import { expect, test } from "./support/test";

test("uses the shared CORS confirmation flow and leaves saves and connection tests independent", async ({
	page,
	request,
}) => {
	await authenticate(page, request);
	await page.route("**/api/v1/admin/policies/test", async (route) => {
		await route.fulfill({
			status: 200,
			json: { code: "success", msg: "success", data: {} },
		});
	});
	let writes = 0;
	await page.route("**/api/v1/admin/policies/action", async (route) => {
		const input = route.request().postDataJSON();
		expect(input.action_id).toBe("configure_azure_blob_cors");
		expect(input.connection.connector_config.values.endpoint).toBe(
			"https://account.blob.core.windows.net",
		);
		expect(input).not.toHaveProperty("preview");
		expect(input).not.toHaveProperty("confirmation");
		writes += 1;
		await route.fulfill({
			status: 200,
			json: {
				code: "success",
				msg: "success",
				data: {
					ok: true,
					action_id: input.action_id,
					output: {
						account: "account",
						service_endpoint: "https://account.blob.core.windows.net/",
						allowed_origins: ["https://drive.example.test"],
						changed: true,
						preserved_rule_count: 2,
						request_id: "azure-request-browser",
					},
				},
			},
		});
	});
	await gotoAdminPage(page, "/admin/policies", "Storage Policies");
	await page.getByRole("button", { name: "New Policy" }).click();
	await page.getByRole("button", { name: "Azure Blob", exact: true }).click();
	await page.getByRole("button", { name: "Next", exact: true }).click();
	await page.locator("#name").fill(uniqueName("azure-cors"));
	await page.locator("#endpoint").fill("https://account.blob.core.windows.net");
	await page.locator("#bucket").fill("archive");
	await page
		.getByLabel("Storage Account Name", { exact: true })
		.fill("account");
	await page
		.getByLabel("Storage Account Key", { exact: true })
		.fill("c2VjcmV0");
	await page
		.getByRole("button", { name: "Test Connection", exact: true })
		.click();
	await expect(
		page.getByText("Connection successful", { exact: true }),
	).toBeVisible();
	expect(writes).toBe(0);
	await page.getByRole("button", { name: "Review", exact: true }).click();
	await page
		.getByRole("button", {
			name: "Auto configure Azure Blob CORS",
			exact: true,
		})
		.click();
	await expect(
		page.getByRole("button", { name: "Confirm action", exact: true }),
	).toBeVisible();
	const configuredOrigin = new URL(page.url()).origin;
	await expect(
		page.getByText(/entire storage account currently used/).last(),
	).toContainText(configuredOrigin);
	await expect(
		page.getByText(/entire storage account currently used/).last(),
	).not.toContainText("{{public_site_url}}");
	expect(writes).toBe(0);
	await page.getByRole("button", { name: "2 Configure Connection" }).click();
	await page.locator("#bucket").fill("changed-container");
	await page.getByRole("button", { name: "Review", exact: true }).click();
	await expect(
		page.getByRole("button", { name: "Confirm action", exact: true }),
	).toHaveCount(0);
	await page
		.getByRole("button", {
			name: "Auto configure Azure Blob CORS",
			exact: true,
		})
		.click();
	await page
		.getByRole("button", { name: "Confirm action", exact: true })
		.click();
	await expect(page.getByText(/azure-request-browser/)).toBeVisible();
	expect(writes).toBe(1);
	await page.getByRole("button", { name: "Create", exact: true }).click();
	await expect(page).toHaveURL(/\/admin\/policies\/\d+$/);
	expect(writes).toBe(1);
});
