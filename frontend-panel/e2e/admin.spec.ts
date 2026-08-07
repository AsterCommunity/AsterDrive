import { createTeamViaApi } from "./support/api";
import { authenticate, gotoAdminPage } from "./support/auth";
import {
	clickRowAction,
	closeActiveDialog,
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
			const detailDialog = dialogByTitle(page, "User details");
			await expect(detailDialog).toBeVisible();
			await expect(
				detailDialog.getByText(email, { exact: true }),
			).toBeVisible();
			await detailDialog
				.locator('[data-slot="dialog-footer"]')
				.getByRole("button", { name: "Close" })
				.click();
			await expect(detailDialog).toBeHidden();

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
			const createDialog = dialogByTitle(page, "Create Policy");
			await expect(createDialog).toBeVisible();
			await createDialog.getByRole("button", { name: "Local" }).click();
			await expect(createDialog.locator("#name")).toBeVisible();
			await createDialog.locator("#name").fill(policyName);
			await createDialog.locator("#base_path").fill(initialBasePath);
			await createDialog
				.getByRole("button", { exact: true, name: "Review" })
				.click();
			await createDialog.getByRole("button", { name: "Create" }).click();
			await expect(createDialog).toBeHidden();

			await expect(tableRowByCellText(page, policyName)).toBeVisible({
				timeout: 30_000,
			});
			await expect(tableRowByCellText(page, policyName)).toContainText(
				initialBasePath,
			);

			await tableRowByCellText(page, policyName).click();
			const editDialog = dialogByTitle(page, "Edit Policy");
			await expect(editDialog).toBeVisible();
			await editDialog.locator("#base_path").fill(updatedBasePath);
			await Promise.all([
				page.waitForResponse(
					(response) =>
						response.request().method() === "PATCH" &&
						response.url().includes("/api/v1/admin/policies/") &&
						response.ok(),
				),
				editDialog.getByRole("button", { name: "Save Changes" }).click(),
			]);
			await expect(editDialog).toBeVisible();
			await closeActiveDialog(page);
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

		test("renders and validates Alibaba Cloud OSS from its connector descriptor", async ({
			page,
			request,
		}) => {
			await authenticate(page, request);

			await gotoAdminPage(page, "/admin/policies", "Storage Policies");
			await page.getByRole("button", { name: "New Policy" }).click();
			const createDialog = dialogByTitle(page, "Create Policy");
			await expect(createDialog).toBeVisible();
			await createDialog
				.getByRole("button", { name: /Alibaba Cloud OSS/ })
				.click();

			const endpoint = createDialog.getByLabel("Public endpoint");
			const serverSideEndpoint = createDialog.getByLabel(
				"Server-side endpoint",
			);
			const region = createDialog.getByLabel("OSS region");
			const bucket = createDialog.getByLabel("Bucket");
			const basePath = createDialog.getByLabel("Base Path");
			const useCname = createDialog.getByRole("switch", {
				name: "Use CNAME custom domain",
			});
			const accessKeyId = createDialog.getByLabel("Alibaba Cloud AccessKey ID");
			const accessKeySecret = createDialog.getByLabel(
				"Alibaba Cloud AccessKey Secret",
			);

			await expect(endpoint).toBeVisible();
			await expect(serverSideEndpoint).toBeVisible();
			await expect(region).toBeVisible();
			await expect(bucket).toBeVisible();
			await expect(basePath).toBeVisible();
			await expect(useCname).not.toBeChecked();
			await expect(
				createDialog.getByLabel("Object Storage Upload Strategy"),
			).toBeVisible();
			await expect(
				createDialog.getByLabel("Object Storage Download Strategy"),
			).toBeVisible();
			await expect(accessKeySecret).toHaveAttribute("type", "password");

			const policyName = uniqueName("pw-oss-policy");
			const testSecret = "playwright-oss-secret";
			await createDialog.locator("#name").fill(policyName);
			await endpoint.fill("http://127.0.0.1:9");
			await region.fill("cn-beijing");
			await bucket.fill("asterdrive-e2e");
			await basePath.fill("e2e/oss");
			await accessKeyId.fill("playwright-access-key");
			await accessKeySecret.fill(testSecret);

			const testConnection = createDialog.getByRole("button", {
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

			await createDialog
				.getByRole("button", { exact: true, name: "Review" })
				.click();
			const summary = createDialog.getByTestId("policy-summary-card");
			await expect(summary).toContainText(policyName);
			await expect(summary).toContainText("http://127.0.0.1:9");
			await expect(summary).toContainText("cn-beijing");
			await expect(summary).toContainText("asterdrive-e2e");
			await expect(summary.getByText(testSecret, { exact: true })).toHaveCount(
				0,
			);

			await closeActiveDialog(page);
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
