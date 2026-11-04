import type { Page } from "@playwright/test";
import { createFolderViaApi, uploadFileViaApi } from "./support/api";
import { authenticate, gotoAdminPage } from "./support/auth";
import { ADMIN, uniqueName } from "./support/fixtures";
import { apiJsonInPage } from "./support/network";
import { expect, test } from "./support/test";

async function chooseSearch(
	page: Page,
	label: string,
	keyword: string,
	option: string,
) {
	await page.getByLabel(label, { exact: true }).click();
	await page.getByPlaceholder("Search by name or email…").fill(keyword);
	await page.getByRole("option").filter({ hasText: option }).click();
	await expect(page.getByLabel(label, { exact: true })).toContainText(option);
	await expect(page.getByRole("listbox")).toBeHidden();
}

test.describe
	.serial("Admin list filters", () => {
		test("debounces files, keeps input selection, and restores policy and owner labels from URL", async ({
			page,
			request,
		}, testInfo) => {
			await authenticate(page, request);
			const name = uniqueName("lookup-policy");
			const policy = await apiJsonInPage<{ id: number }>(
				page,
				"/api/v1/admin/policies",
				{
					method: "POST",
					withCsrf: true,
					body: {
						name,
						connection: {
							connector_config: {
								format_version: 1,
								connector_id: "asterdrive.storage.local",
								schema_version: 1,
								values: { base_path: `/tmp/${name}`, content_dedup: false },
							},
							credential: { mode: "none" },
							behavior: {},
						},
					},
				},
			);
			await gotoAdminPage(page, "/admin/files?offset=20", "Files");
			await page.getByRole("button", { name: "Filters", exact: true }).click();
			const calls: string[] = [];
			page.on("request", (request) => {
				const url = new URL(request.url());
				if (url.pathname === "/api/v1/admin/files") calls.push(url.search);
			});
			const input = page.getByPlaceholder("Filter by name");
			await input.pressSequentially("report", { delay: 15 });
			await input.press("Home");
			await input.press("ArrowRight");
			await input.press("ArrowRight");
			await expect(page).toHaveURL(/name=report/);
			expect(calls).toHaveLength(1);
			expect(new URL(page.url()).searchParams.has("offset")).toBe(false);
			expect(
				await input.evaluate(
					(element: HTMLInputElement) => element.selectionStart,
				),
			).toBe(2);
			await chooseSearch(page, "Storage policy", name, name);
			await chooseSearch(page, "File owner", ADMIN.email, ADMIN.username);
			await page.reload();
			await page.getByRole("button", { name: /Filters/ }).click();
			await expect(
				page.getByLabel("Storage policy", { exact: true }),
			).toContainText(name);
			await expect(
				page.getByLabel("File owner", { exact: true }),
			).toContainText(ADMIN.username);
			expect(new URL(page.url()).searchParams.get("policyId")).toBe(
				String(policy.id),
			);
			await expect(
				page
					.locator("[aria-hidden=false]")
					.filter({ has: page.getByPlaceholder("Filter by name") }),
			).toHaveCSS("max-height", "none");
			await page.screenshot({
				path: testInfo.outputPath("files.png"),
				fullPage: true,
			});
		});

		test("combines Blob units, uploader, health and hash type and reproduces the URL", async ({
			page,
			request,
		}, testInfo) => {
			await authenticate(page, request);
			const uploaded = await uploadFileViaApi(page, "/api/v1", {
				name: `${uniqueName("blob-filter")}.txt`,
				mimeType: "text/plain",
				buffer: Buffer.alloc(2048, "a"),
			});
			const fileDetail = await apiJsonInPage<{ blob_id: number }>(
				page,
				`/api/v1/admin/files/${uploaded.id}`,
			);
			const blobDetail = await apiJsonInPage<{
				hash_kind: string;
				health: string;
			}>(page, `/api/v1/admin/file-blobs/${fileDetail.blob_id}`);
			const hashLabel =
				blobDetail.hash_kind === "content_sha256"
					? "Content SHA-256"
					: "Opaque";
			await gotoAdminPage(page, "/admin/file-blobs", "File Blobs");
			await page.getByRole("button", { name: "Filters", exact: true }).click();
			await page.getByLabel("Size unit", { exact: true }).first().click();
			await page.getByRole("option", { name: "KB", exact: true }).click();
			await page.getByPlaceholder("Min size").fill("1.5");
			await page.getByPlaceholder("Max size").fill("3");
			await expect(page).toHaveURL(/sizeMin=1536/);
			await expect(page).toHaveURL(/sizeMax=3072/);
			await page.getByLabel("Health", { exact: true }).click();
			await page.getByRole("option", { name: "Healthy", exact: true }).click();
			await page.getByLabel("Hash kind", { exact: true }).click();
			await page.getByRole("option", { name: hashLabel, exact: true }).click();
			await chooseSearch(page, "Uploader", ADMIN.email, ADMIN.username);
			const url = page.url();
			const response = await apiJsonInPage<{ total: number }>(
				page,
				`/api/v1/admin/file-blobs?size_min=1536&size_max=3072&health=healthy&hash_kind=${blobDetail.hash_kind}&uploader_user_id=${new URL(url).searchParams.get("uploaderUserId")}`,
			);
			expect(response.total).toBeGreaterThan(0);
			await page.goto(url);
			await page.getByRole("button", { name: /Filters/ }).click();
			await expect(page.getByPlaceholder("Min size")).toHaveValue("1.5");
			await expect(page.getByPlaceholder("Max size")).toHaveValue("3");
			await expect(page.getByLabel("Health", { exact: true })).toContainText(
				"Healthy",
			);
			await expect(page.getByLabel("Hash kind", { exact: true })).toContainText(
				hashLabel,
			);
			await expect(page.getByLabel("Uploader", { exact: true })).toContainText(
				ADMIN.username,
			);
			await expect(
				page
					.locator("[aria-hidden=false]")
					.filter({ has: page.getByPlaceholder("Min size") }),
			).toHaveCSS("max-height", "none");
			await page.screenshot({
				path: testInfo.outputPath("blobs.png"),
				fullPage: true,
			});
			await page.setViewportSize({ width: 390, height: 844 });
			await page.getByRole("button", { name: /Filters/, exact: false }).click();
			await expect(page.getByPlaceholder("Min size")).toBeVisible();
			await expect(
				page
					.locator("[aria-hidden=false]")
					.filter({ has: page.getByPlaceholder("Min size") }),
			).toHaveCSS("max-height", "none");
			await page.screenshot({
				path: testInfo.outputPath("blobs-mobile.png"),
				fullPage: true,
			});
			await page
				.getByRole("button", { name: "Clear filters", exact: true })
				.click();
			expect(new URL(page.url()).search).toBe("");
			await expect(page.getByPlaceholder("Min size")).toHaveValue("");
		});

		test("filters file and folder shares by user and shows batch target names", async ({
			page,
			request,
		}, testInfo) => {
			await authenticate(page, request);
			const filename = `${uniqueName("share-filter")}.txt`;
			const foldername = uniqueName("folder-filter");
			const file = await uploadFileViaApi(page, "/api/v1", {
				name: filename,
				mimeType: "text/plain",
				buffer: Buffer.from(filename),
			});
			const folder = await createFolderViaApi(page, "/api/v1", foldername);
			for (const target of [
				{ type: "file", id: file.id },
				{ type: "folder", id: folder.id },
			])
				await apiJsonInPage(page, "/api/v1/shares", {
					method: "POST",
					withCsrf: true,
					body: { target },
				});
			await gotoAdminPage(page, "/admin/shares", "Shares");
			await expect(
				page.getByRole("cell", { name: filename, exact: true }),
			).toBeVisible();
			await expect(
				page.getByRole("cell", { name: foldername, exact: true }),
			).toBeVisible();
			await page.getByRole("button", { name: "Filters", exact: true }).click();
			await page.getByLabel("Type", { exact: true }).click();
			await page.getByRole("option", { name: "Folder", exact: true }).click();
			await chooseSearch(page, "Share user", ADMIN.email, ADMIN.username);
			await expect(
				page.getByRole("cell", { name: filename, exact: true }),
			).toHaveCount(0);
			await expect(
				page.getByRole("cell", { name: foldername, exact: true }),
			).toBeVisible();
			const url = page.url();
			await page.goto(url);
			await expect(
				page.getByRole("cell", { name: foldername, exact: true }),
			).toBeVisible();
			await page.getByRole("button", { name: /Filters/ }).click();
			await expect(
				page.getByLabel("Share user", { exact: true }),
			).toContainText(ADMIN.username);
			await expect(
				page
					.locator("[aria-hidden=false]")
					.filter({ has: page.getByLabel("Share user", { exact: true }) }),
			).toHaveCSS("max-height", "none");
			await page.screenshot({
				path: testInfo.outputPath("shares.png"),
				fullPage: true,
			});
			await page
				.getByRole("button", { name: "Clear filters", exact: true })
				.click();
			expect(new URL(page.url()).search).toBe("");
		});
	});
