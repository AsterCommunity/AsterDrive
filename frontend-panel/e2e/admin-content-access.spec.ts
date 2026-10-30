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

async function beginAccess(
	page: import("@playwright/test").Page,
	remark = "Support case 628",
) {
	await page.getByLabel("Remark (optional, up to 500 characters)").fill(remark);
	await page.getByRole("combobox", { name: "Access reason" }).click();
	await page.getByRole("option", { name: "User support", exact: true }).click();
	await page.getByRole("button", { name: "Begin read-only access" }).click();
}

test("admin inspects personal/team workspaces and downloads current/revision content through native browser", async ({
	page: adminPage,
	request,
}) => {
	let page = adminPage;
	await page.context().addInitScript(() => {
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
	await page.goto(`/admin/users/${me.id}`);
	await resolveAdminSiteUrlPrompt(page);
	const sidebar = page.locator("aside").filter({
		has: page.getByRole("button", { name: "View workspace files" }),
	});
	await expect(
		sidebar.getByRole("button", { name: "View workspace files" }),
	).toBeVisible();
	await expect(
		page.getByRole("navigation", { name: "User details" }),
	).toHaveCount(0);
	await page.screenshot({ path: "/private/tmp/admin-628-user-sidebar.png" });
	const opened = adminPage.context().waitForEvent("page");
	await sidebar.getByRole("button", { name: "View workspace files" }).click();
	page = await opened;
	await page.waitForLoadState("domcontentloaded");
	await expect(adminPage).toHaveURL(new RegExp(`/admin/users/${me.id}$`));
	await expect(page).toHaveURL(new RegExp(`/admin/users/${me.id}/files`));
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
	await page.getByRole("button", { name: "List View", exact: true }).click();
	await expect(page.getByRole("cell", { name, exact: true })).toBeVisible();
	await page.getByRole("button", { name: "Grid View", exact: true }).click();
	await expect(
		page.locator("[data-file-list-item]").filter({ hasText: name }),
	).toBeVisible();
	await page.getByRole("button", { name: "List View", exact: true }).click();
	await expect(
		page.getByRole("button", {
			name: /Upload|Rename|Delete|Share/,
			exact: true,
		}),
	).toHaveCount(0);
	await page.screenshot({
		path: "/private/tmp/admin-628-workspace.png",
		animations: "disabled",
	});

	await page
		.getByRole("row")
		.filter({ hasText: name })
		.getByRole("button", { name: "More actions" })
		.click();
	await expect(
		page.getByRole("menuitem", { name: /Rename|Delete|Share|Move|Copy|Lock/ }),
	).toHaveCount(0);
	const [download] = await Promise.all([
		page.waitForEvent("download"),
		page.getByRole("menuitem", { name: "Download", exact: true }).click(),
	]);
	expect(download.suggestedFilename()).toBe(name);
	const saved = await download.path();
	expect(saved).not.toBeNull();
	expect(await readFile(saved as string)).toEqual(original);
	await page.getByRole("cell", { name, exact: true }).click();

	await expect(
		page.getByRole("complementary", { name: "Details" }),
	).toBeVisible();
	await expect(page).toHaveURL(
		new RegExp(`/admin/users/${me.id}/files.*fileId=${file.id}`),
	);
	const inspectionCount = (
		await apiJsonInPage<{ total: number }>(
			page,
			"/api/v1/admin/audit-logs?action=admin_workspace_inspect",
		)
	).total;
	await page.getByRole("button", { name: "Versions", exact: true }).click();
	const history = page.getByRole("dialog");
	await expect(
		history.getByRole("heading", { name: `Version History - ${name}` }),
	).toBeVisible();
	const [historical] = await Promise.all([
		page.waitForEvent("download"),
		history
			.getByRole("button", { name: "Download revision v1", exact: true })
			.click(),
	]);
	expect(await readFile((await historical.path()) as string)).toEqual(original);
	expect(
		(
			await apiJsonInPage<{ total: number }>(
				page,
				"/api/v1/admin/audit-logs?action=admin_workspace_inspect",
			)
		).total,
	).toBe(inspectionCount);
	const detail = await apiJsonInPage<AdminFileDetail>(
		page,
		`/api/v1/admin/files/${file.id}`,
	);
	const revision = detail.versions[0];

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
	await page.goto("/admin/audit?action=admin_workspace_inspect");
	await expect(
		page
			.getByRole("row")
			.filter({ hasText: "User support" })
			.filter({ hasText: "Support case 628" })
			.first(),
	).toContainText(`(#${me.id})`);
	await page.screenshot({ path: "/private/tmp/admin-628-audit-readable.png" });
	await page.addInitScript(() => localStorage.setItem("aster-language", "zh"));
	await page.reload();
	const localized = page
		.getByRole("row")
		.filter({ hasText: "原因 用户支持" })
		.filter({ hasText: "备注：Support case 628" })
		.first();
	await expect(localized).toContainText("已授权查看");
	await expect(localized).toContainText(`(#${me.id})`);
	await page.screenshot({
		path: "/private/tmp/admin-628-audit-readable-zh.png",
		animations: "disabled",
	});
});
