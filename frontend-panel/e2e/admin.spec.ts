import { createTeamViaApi } from "./support/api";
import { authenticate, gotoAdminPage } from "./support/auth";
import {
	clickRowAction,
	createPageShare,
	dialogByTitle,
	fileNameCell,
	tableRowByCellText,
	uploadViaPicker,
} from "./support/files";
import { uniqueAccountName, uniqueName } from "./support/fixtures";
import { expect, test } from "./support/test";

test.describe
	.serial("Admin E2E", () => {
		test("manages admin users end-to-end", async ({ page, request }) => {
			await authenticate(page, request);

			const username = uniqueAccountName("pwuser");
			const email = `${username}@example.com`;

			await gotoAdminPage(page, "/admin/users", "Users");

			await page.getByRole("button", { name: "New User" }).click();
			const createDialog = dialogByTitle(page, "Create user");
			await expect(createDialog).toBeVisible();
			await createDialog.locator("#create-user-username").fill(username);
			await createDialog.locator("#create-user-email").fill(email);
			await createDialog
				.locator("#create-user-password")
				.fill("Playwright123!");
			await createDialog.getByRole("button", { name: "Create" }).click();
			await expect(createDialog).toBeHidden();

			await expect(tableRowByCellText(page, username)).toBeVisible({
				timeout: 30_000,
			});

			await page.getByRole("button", { name: "Filters" }).click();
			await page
				.getByPlaceholder("Search by username, email, or ID...")
				.fill(username);
			await expect(tableRowByCellText(page, username)).toBeVisible({
				timeout: 30_000,
			});

			await tableRowByCellText(page, username).click();
			await expect(page).toHaveURL(/\/admin\/users\/\d+/);
			await expect(page.getByText(email, { exact: true })).toBeVisible();
			await page
				.getByRole("button", { name: "Back", exact: true })
				.first()
				.click();
			await expect(page).toHaveURL(/\/admin\/users$/);

			await clickRowAction(tableRowByCellText(page, username), "Delete user");
			const deleteDialog = page.getByRole("alertdialog", {
				name: "Delete user",
			});
			await expect(deleteDialog).toBeVisible();
			await deleteDialog.getByRole("button", { name: "Delete" }).click();
			await expect(tableRowByCellText(page, username)).toHaveCount(0, {
				timeout: 30_000,
			});
		});

		test("configures local storage policies through the admin flow", async ({
			page,
			request,
		}) => {
			await authenticate(page, request);

			const policyName = uniqueName("pw-local-policy");
			const initialBasePath = `/tmp/${policyName}-v1`;
			const updatedBasePath = `/tmp/${policyName}-v2`;

			await gotoAdminPage(page, "/admin/policies", "Storage Policies");

			await page.getByRole("button", { name: "New Policy" }).click();
			await expect(page).toHaveURL(/\/admin\/policies\/new/);
			await page.getByRole("button", { name: "Local" }).click();
			await expect(page.locator("#name")).toBeVisible();
			await page.locator("#name").fill(policyName);
			await page.locator("#base_path").fill(initialBasePath);
			await page.getByRole("button", { exact: true, name: "Review" }).click();
			await page.getByRole("button", { name: "Create" }).click();
			await expect(page).toHaveURL(/\/admin\/policies$/);

			await expect(tableRowByCellText(page, policyName)).toBeVisible({
				timeout: 30_000,
			});
			await expect(tableRowByCellText(page, policyName)).toContainText(
				initialBasePath,
			);

			await tableRowByCellText(page, policyName).click();
			await expect(page).toHaveURL(/\/admin\/policies\/\d+/);
			await expect(page.locator("#base_path")).toBeVisible();
			await page.locator("#base_path").fill(updatedBasePath);
			await Promise.all([
				page.waitForResponse(
					(response) =>
						response.request().method() === "PATCH" &&
						response.url().includes("/api/v1/admin/policies/") &&
						response.ok(),
				),
				page.getByRole("button", { name: "Save Changes" }).click(),
			]);
			await page
				.getByRole("button", { name: "Policies", exact: true })
				.first()
				.click();
			await expect(page).toHaveURL(/\/admin\/policies$/);
			await expect(tableRowByCellText(page, policyName)).toContainText(
				updatedBasePath,
			);

			await clickRowAction(
				tableRowByCellText(page, policyName),
				"Delete Policy",
			);
			const deleteDialog = page.getByRole("alertdialog", {
				name: `Delete Policy "${policyName}"?`,
			});
			await expect(deleteDialog).toBeVisible();
			await deleteDialog.getByRole("button", { name: "Delete" }).click();
			await expect(tableRowByCellText(page, policyName)).toHaveCount(0, {
				timeout: 30_000,
			});
		});

		test("simulates policy group routing through the admin flow", async ({
			page,
			request,
		}) => {
			await authenticate(page, request);
			await gotoAdminPage(page, "/admin/policy-groups", "Policy Groups");

			const policyGroupRow = page
				.getByRole("row")
				.filter({
					has: page.getByRole("button", { name: "Simulate" }),
				})
				.first();
			await expect(policyGroupRow).toBeVisible({ timeout: 30_000 });
			await clickRowAction(policyGroupRow, "Simulate");

			const simulationDialog = dialogByTitle(page, "Routing Simulation");
			await expect(simulationDialog).toBeVisible();
			await simulationDialog.getByLabel("Filename").fill("archive.tar.gz");
			await simulationDialog.getByLabel("File size (MB)").fill("2");

			const [simulationResponse] = await Promise.all([
				page.waitForResponse(
					(response) =>
						response.request().method() === "POST" &&
						response.url().includes("/api/v1/admin/policy-groups/") &&
						response.url().endsWith("/simulate"),
				),
				simulationDialog
					.getByRole("button", { name: "Run Simulation" })
					.click(),
			]);
			expect(simulationResponse.ok()).toBe(true);
			await expect(simulationDialog.getByText("Target selected")).toBeVisible();
			await expect(
				simulationDialog.getByText("Admission passed"),
			).toBeVisible();
			await expect(
				simulationDialog.getByText("archive", { exact: true }),
			).toBeVisible();
			await expect(
				simulationDialog.getByText("tar.gz", { exact: true }),
			).toBeVisible();
		});

		test("renders and validates Alibaba Cloud OSS from its connector descriptor", async ({
			page,
			request,
		}) => {
			await authenticate(page, request);

			await gotoAdminPage(page, "/admin/policies", "Storage Policies");
			await page.getByRole("button", { name: "New Policy" }).click();
			await expect(page).toHaveURL(/\/admin\/policies\/new/);
			await page.getByRole("button", { name: /Alibaba Cloud OSS/ }).click();

			const endpoint = page.getByLabel("Public endpoint");
			const serverSideEndpoint = page.getByLabel("Server-side endpoint");
			const region = page.getByLabel("OSS region");
			const bucket = page.getByLabel("Bucket");
			const basePath = page.getByLabel("Base Path");
			const useCname = page.getByRole("switch", {
				name: "Use CNAME custom domain",
			});
			const accessKeyId = page.getByLabel("Alibaba Cloud AccessKey ID");
			const accessKeySecret = page.getByLabel("Alibaba Cloud AccessKey Secret");

			await expect(endpoint).toBeVisible();
			await expect(serverSideEndpoint).toBeVisible();
			await expect(region).toBeVisible();
			await expect(bucket).toBeVisible();
			await expect(basePath).toBeVisible();
			await expect(useCname).not.toBeChecked();
			await expect(
				page.getByLabel("Object Storage Upload Strategy"),
			).toBeVisible();
			await expect(
				page.getByLabel("Object Storage Download Strategy"),
			).toBeVisible();
			await expect(accessKeySecret).toHaveAttribute("type", "password");

			const policyName = uniqueName("pw-oss-policy");
			const testSecret = "playwright-oss-secret";
			await page.locator("#name").fill(policyName);
			await endpoint.fill("http://127.0.0.1:9");
			await region.fill("cn-beijing");
			await bucket.fill("asterdrive-e2e");
			await basePath.fill("e2e/oss");
			await accessKeyId.fill("playwright-access-key");
			await accessKeySecret.fill(testSecret);

			const testConnection = page.getByRole("button", {
				name: "Test Connection",
			});
			const [invalidEndpointResponse] = await Promise.all([
				page.waitForResponse(
					(response) =>
						response.request().method() === "POST" &&
						response.url().endsWith("/api/v1/admin/policies/test"),
				),
				testConnection.click(),
			]);
			expect(invalidEndpointResponse.ok()).toBe(false);
			expect(await invalidEndpointResponse.text()).toContain(
				"unless CNAME mode is enabled",
			);

			const testPayload = invalidEndpointResponse.request().postDataJSON() as {
				connection: {
					connector_config: {
						connector_id: string;
						values: Record<string, unknown>;
					};
					credential: {
						mode: string;
						values: Record<string, string>;
					};
				};
			};
			expect(testPayload.connection.connector_config).toMatchObject({
				connector_id: "asterdrive.storage.alibaba_oss",
				values: {
					base_path: "e2e/oss",
					bucket: "asterdrive-e2e",
					endpoint: "http://127.0.0.1:9",
					oss_region: "cn-beijing",
					oss_use_cname: false,
				},
			});
			expect(testPayload.connection.credential).toMatchObject({
				mode: "static",
				values: {
					aliyun_oss_access_key_id: "playwright-access-key",
					aliyun_oss_access_key_secret: testSecret,
				},
			});

			await useCname.click();
			await expect(useCname).toBeChecked();
			const [cnameResponse] = await Promise.all([
				page.waitForResponse(
					(response) =>
						response.request().method() === "POST" &&
						response.url().endsWith("/api/v1/admin/policies/test"),
				),
				testConnection.click(),
			]);
			expect(cnameResponse.ok()).toBe(false);
			expect(await cnameResponse.text()).not.toContain(
				"unless CNAME mode is enabled",
			);

			await page.getByRole("button", { exact: true, name: "Review" }).click();
			const summary = page.getByTestId("policy-summary-card");
			await expect(summary).toContainText(policyName);
			await expect(summary).toContainText("http://127.0.0.1:9");
			await expect(summary).toContainText("cn-beijing");
			await expect(summary).toContainText("asterdrive-e2e");
			await expect(summary.getByText(testSecret, { exact: true })).toHaveCount(
				0,
			);

			await page
				.getByRole("button", { name: "Policies", exact: true })
				.first()
				.click();
			await expect(page).toHaveURL(/\/admin\/policies$/);
		});

		test("surfaces team and share records in admin pages", async ({
			page,
			request,
		}) => {
			await authenticate(page, request);

			const teamName = uniqueName("pw-admin-team");
			const team = await createTeamViaApi(
				page,
				teamName,
				"Team created for admin E2E coverage",
			);
			const sharedFile = {
				buffer: Buffer.from("admin share coverage\n", "utf8"),
				mimeType: "text/plain",
				name: `${uniqueName("pw-admin-share")}.txt`,
			} as const;

			await page.goto("/");
			await uploadViaPicker(page, [sharedFile]);
			await expect(fileNameCell(page, sharedFile.name)).toBeVisible({
				timeout: 30_000,
			});
			const shareUrl = await createPageShare(page, sharedFile.name);
			const shareToken = shareUrl.split("/s/").at(-1) ?? "";
			expect(shareToken.length).toBeGreaterThan(0);

			await gotoAdminPage(
				page,
				`/admin/teams?keyword=${encodeURIComponent(teamName)}`,
				"Teams",
			);
			const teamRow = page
				.getByRole("row")
				.filter({ hasText: teamName })
				.first();
			await expect(teamRow).toBeVisible({
				timeout: 30_000,
			});
			await teamRow.click();
			await expect(page).toHaveURL(
				new RegExp(`/admin/teams/${team.id}/overview$`),
			);
			await expect(page.locator("#admin-team-detail-name")).toHaveValue(
				teamName,
				{
					timeout: 30_000,
				},
			);

			await page.getByRole("tab", { name: "Members" }).click();
			await expect(page).toHaveURL(
				new RegExp(`/admin/teams/${team.id}/members$`),
			);
			await expect(
				page.getByRole("row").filter({ hasText: "admin@example.com" }).first(),
			).toBeVisible({
				timeout: 30_000,
			});

			await gotoAdminPage(page, "/admin/shares", "Shares");
			const shareRow = tableRowByCellText(page, shareToken);
			await expect(shareRow).toBeVisible({ timeout: 30_000 });
			await shareRow.getByRole("button").last().click();
			const deleteDialog = page.getByRole("alertdialog");
			await expect(deleteDialog).toBeVisible();
			await deleteDialog
				.getByRole("button", { exact: true, name: "Delete" })
				.click();
			await expect(shareRow).toHaveCount(0, { timeout: 30_000 });
		});
	});
