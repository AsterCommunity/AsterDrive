import { authenticate } from "./support/auth";
import { dialogByTitle, tableRowByCellText } from "./support/files";
import { uniqueAccountName, uniqueName } from "./support/fixtures";
import {
	apiJsonInPage,
	basicAuth,
	normalizeWebdavPrefix,
	webdavRequest,
} from "./support/network";
import { expect, test } from "./support/test";

test.describe
	.serial("WebDAV E2E", () => {
		test("creates a WebDAV account and exercises basic WebDAV methods", async ({
			page,
			request,
		}) => {
			await authenticate(page, request);

			const username = uniqueAccountName("pwdav");
			const password = "PlaywrightDav123!";
			const settings = await apiJsonInPage<{
				endpoint: string;
				prefix: string;
			}>(page, "/api/v1/webdav-accounts/settings");
			const prefix = normalizeWebdavPrefix(settings.prefix);
			const authHeader = basicAuth(username, password);
			const directoryName = uniqueName("pw-dav-dir");
			const fileName = `${uniqueName("pw-dav-file")}.txt`;
			const copiedFileName = `${uniqueName("pw-dav-copy")}.txt`;
			const movedFileName = `${uniqueName("pw-dav-moved")}.txt`;
			const fileContent = "WebDAV content from Playwright";
			const rootPath = `${prefix}/`;
			const directoryPath = `${rootPath}${directoryName}/`;
			const filePath = `${directoryPath}${fileName}`;
			const copiedFilePath = `${directoryPath}${copiedFileName}`;
			const movedFilePath = `${directoryPath}${movedFileName}`;

			await page.goto("/settings/webdav");
			await expect(
				page.getByRole("heading", { exact: true, name: "WebDAV" }),
			).toBeVisible();
			await page.getByRole("button", { name: "Create WebDAV Account" }).click();

			const createDialog = dialogByTitle(page, "Create WebDAV Account");
			await expect(createDialog).toBeVisible();
			await createDialog.locator("#username").fill(username);
			await createDialog.locator("#password").fill(password);
			await createDialog.getByRole("button", { name: "Create" }).click();

			const credentialsDialog = dialogByTitle(page, "Latest Credentials");
			await expect(credentialsDialog).toBeVisible();
			await credentialsDialog
				.getByRole("button", { name: "Test Connection" })
				.click();
			await expect(
				credentialsDialog.getByText("Connection successful", {
					exact: true,
				}),
			).toBeVisible({
				timeout: 30_000,
			});

			const rootListing = await webdavRequest(page, rootPath, {
				headers: {
					Authorization: authHeader,
					Depth: "0",
				},
				method: "PROPFIND",
			});
			expect(rootListing.status).toBe(207);

			const createDirectory = await webdavRequest(page, directoryPath, {
				headers: {
					Authorization: authHeader,
				},
				method: "MKCOL",
			});
			expect(createDirectory.status).toBe(201);

			const nestedListing = await webdavRequest(page, rootPath, {
				headers: {
					Authorization: authHeader,
					Depth: "1",
				},
				method: "PROPFIND",
			});
			expect(nestedListing.status).toBe(207);
			expect(nestedListing.text).toContain(directoryName);

			const putFile = await webdavRequest(page, filePath, {
				body: fileContent,
				headers: {
					Authorization: authHeader,
					"Content-Type": "text/plain",
				},
				method: "PUT",
			});
			expect([201, 204]).toContain(putFile.status);

			const getFile = await webdavRequest(page, filePath, {
				headers: {
					Authorization: authHeader,
				},
				method: "GET",
			});
			expect(getFile.status).toBe(200);
			expect(getFile.text).toContain(fileContent);

			const copyFile = await webdavRequest(page, filePath, {
				headers: {
					Authorization: authHeader,
					Destination: copiedFilePath,
				},
				method: "COPY",
			});
			expect([201, 204]).toContain(copyFile.status);

			const moveFile = await webdavRequest(page, copiedFilePath, {
				headers: {
					Authorization: authHeader,
					Destination: movedFilePath,
				},
				method: "MOVE",
			});
			expect([201, 204]).toContain(moveFile.status);

			const getMovedFile = await webdavRequest(page, movedFilePath, {
				headers: {
					Authorization: authHeader,
				},
				method: "GET",
			});
			expect(getMovedFile.status).toBe(200);
			expect(getMovedFile.text).toContain(fileContent);

			const lockResponse = await webdavRequest(page, filePath, {
				body: [
					'<?xml version="1.0" encoding="utf-8" ?>',
					'<D:lockinfo xmlns:D="DAV:">',
					"<D:lockscope><D:exclusive/></D:lockscope>",
					"<D:locktype><D:write/></D:locktype>",
					"<D:owner><D:href>playwright</D:href></D:owner>",
					"</D:lockinfo>",
				].join(""),
				headers: {
					Authorization: authHeader,
					"Content-Type": "application/xml",
					Depth: "0",
					Timeout: "Second-600",
				},
				method: "LOCK",
			});
			expect(lockResponse.status).toBe(200);
			expect(lockResponse.text).toContain("locktoken");

			await page.goto("/admin/locks");
			await expect(
				page.getByRole("heading", { exact: true, name: "WebDAV Locks" }),
			).toBeVisible({ timeout: 30_000 });
			const lockRow = tableRowByCellText(page, `/${directoryName}/${fileName}`);
			await expect(lockRow).toBeVisible({ timeout: 30_000 });
			await lockRow.getByRole("button").last().click();
			const unlockDialog = page.getByRole("alertdialog");
			await expect(unlockDialog).toBeVisible();
			await unlockDialog
				.getByRole("button", { exact: true, name: "Confirm" })
				.click();
			await expect(lockRow).toHaveCount(0, { timeout: 30_000 });

			const deleteMovedFile = await webdavRequest(page, movedFilePath, {
				headers: {
					Authorization: authHeader,
				},
				method: "DELETE",
			});
			expect([200, 204]).toContain(deleteMovedFile.status);

			const deleteFile = await webdavRequest(page, filePath, {
				headers: {
					Authorization: authHeader,
				},
				method: "DELETE",
			});
			expect([200, 204]).toContain(deleteFile.status);

			const deleteDirectory = await webdavRequest(page, directoryPath, {
				headers: {
					Authorization: authHeader,
				},
				method: "DELETE",
			});
			expect([200, 204]).toContain(deleteDirectory.status);
		});
	});
