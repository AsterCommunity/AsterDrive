import { readFile } from "node:fs/promises";
import type { AdminFileDetail, UserInfo } from "@/types/api";
import {
	createFolderViaApi,
	createTeamViaApi,
	uploadFileViaApi,
} from "./support/api";
import { authenticate, resolveAdminSiteUrlPrompt } from "./support/auth";
import { uniqueName } from "./support/fixtures";
import { apiJsonInPage } from "./support/network";
import { expect, test } from "./support/test";

async function beginAccess(page: import("@playwright/test").Page) {
	await page.getByRole("combobox", { name: "Access reason" }).click();
	await page.getByRole("option", { name: "User support", exact: true }).click();
	await page.getByRole("button", { name: "Begin read-only access" }).click();
}

test("admin inspects personal/team workspaces and downloads current/revision content through native browser", async ({
	page,
	request,
}) => {
	await page.addInitScript(() => {
		const isContentDownload = (url: string) =>
			url.includes("/admin/content-access/") && url.includes("/download");
		const fetch = window.fetch;
		window.fetch = function (input, init) {
			const url =
				typeof input === "string"
					? input
					: input instanceof URL
						? input.href
						: input.url;
			if (isContentDownload(url))
				throw new Error("admin attachment fetched by JavaScript");
			return fetch.call(this, input, init);
		};
		const open = XMLHttpRequest.prototype.open;
		XMLHttpRequest.prototype.open = function (
			method,
			url,
			async = true,
			username,
			password,
		) {
			if (isContentDownload(String(url)))
				throw new Error("admin attachment requested by XMLHttpRequest");
			return open.call(this, method, url, async, username, password);
		};

		const blob = Response.prototype.blob;
		Response.prototype.blob = function () {
			if (
				this.url.includes("content-access/") &&
				this.url.includes("/download")
			)
				throw new Error("admin download buffered into Blob");
			return blob.call(this);
		};
	});
	await authenticate(page, request);
	const me = await apiJsonInPage<UserInfo>(page, "/api/v1/auth/me");
	const folder = await createFolderViaApi(
		page,
		"/api/v1",
		uniqueName("inspect-folder"),
	);
	const name = `${uniqueName("admin-inspect")}.txt`;
	const original = Buffer.from("administrator recovery bytes\n");
	const file = await uploadFileViaApi(
		page,
		"/api/v1",
		{ name, mimeType: "text/plain", buffer: original },
		folder.id,
	);
	await apiJsonInPage(
		page,
		"/api/v1/admin/config/admin_workspace_content_access_enabled",
		{ method: "PUT", withCsrf: true, body: { value: "false" } },
	);
	await page.goto(`/admin/users/${me.id}/files?folderId=${folder.id}`);
	await resolveAdminSiteUrlPrompt(page);
	await expect(
		page.getByText("Administrator content access is disabled", { exact: true }),
	).toBeVisible();
	await expect(page.getByRole("table")).toHaveCount(0);
	await apiJsonInPage(
		page,
		"/api/v1/admin/config/admin_workspace_content_access_enabled",
		{ method: "PUT", withCsrf: true, body: { value: "true" } },
	);
	await page.reload();
	await beginAccess(page);
	await expect(page.getByRole("link", { name, exact: true })).toBeVisible();
	await expect(
		page.getByRole("button", {
			name: /Upload|Rename|Delete|Share/,
			exact: true,
		}),
	).toHaveCount(0);
	await page.screenshot({ path: "/private/tmp/admin-628-workspace.png" });

	const [download] = await Promise.all([
		page.waitForEvent("download"),
		page
			.getByRole("button", { name: "Download current version", exact: true })
			.click(),
	]);
	expect(download.suggestedFilename()).toBe(name);
	const saved = await download.path();
	expect(saved).not.toBeNull();
	expect(await readFile(saved as string)).toEqual(original);
	await page.getByRole("link", { name, exact: true }).click();
	const dialog = page.getByRole("dialog");
	await expect(dialog).toBeVisible();
	const detail = await apiJsonInPage<AdminFileDetail>(
		page,
		`/api/v1/admin/files/${file.id}`,
	);
	const revision = detail.versions[0];
	expect(revision).toBeDefined();
	await page.goto(`/admin/files?fileId=${file.id}&revisionId=${revision.id}`);
	await expect(page.getByRole("dialog")).toBeVisible();
	await page
		.getByRole("button", { name: "Access and download content" })
		.click();
	await beginAccess(page);
	const [historical] = await Promise.all([
		page.waitForEvent("download"),
		page
			.getByRole("button", { name: `Download revision v${revision.version}` })
			.click(),
	]);
	expect(await readFile((await historical.path()) as string)).toEqual(original);

	// Blob observability hands off to the same logical revision URL.
	await page.goto(
		`/admin/file-blobs?hash=${encodeURIComponent(detail.blob.hash)}`,
	);
	await page
		.getByRole("row")
		.filter({ hasText: String(detail.blob.id) })
		.first()
		.click();
	await page
		.getByRole("link", { name: new RegExp(`v${revision.version}`) })
		.click();
	await expect(page).toHaveURL(
		new RegExp(`fileId=${file.id}.*revisionId=${revision.id}`),
	);
	await expect(page.getByRole("dialog")).toBeVisible();
	const team = await createTeamViaApi(page, uniqueName("inspect-team"));
	await page.goto(`/admin/teams/${team.id}/files`);
	await beginAccess(page);
	await expect(
		page.getByText("No matching items in this folder", { exact: true }),
	).toBeVisible();
	await page.reload();
	await expect(
		page.getByRole("button", { name: "Begin read-only access" }),
	).toBeDisabled();
});
