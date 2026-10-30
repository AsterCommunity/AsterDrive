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
	remark: string,
) {
	await page.getByLabel("Remark (optional, up to 500 characters)").fill(remark);
	await page.getByRole("combobox", { name: "Access reason" }).click();
	await page.getByRole("option", { name: "User support", exact: true }).click();
	await page.getByRole("button", { name: "Begin read-only access" }).click();
}

test("admin inspects personal/team workspaces and downloads current/revision content through native browser", async ({
	page: adminPage,
	request,
}, testInfo) => {
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
	const personalRemark = uniqueName("Support case 628 personal");
	const teamRemark = uniqueName("Support case 628 team");
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

	const imageName = `${uniqueName("inspection-thumbnail")}.png`;
	await uploadFileViaApi(
		page,
		"/api/v1",
		{
			name: imageName,
			mimeType: "image/png",
			buffer: Buffer.from(
				"iVBORw0KGgoAAAANSUhEUgAAACAAAAAYCAIAAAAUMWhjAAAAJElEQVR4nGP4HxpKU8QwasGoBaMWjFowasGoBaMWjFowNCwAAJ7w+z3kabdcAAAAAElFTkSuQmCC",
				"base64",
			),
		},
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
	await page.screenshot({
		path: testInfo.outputPath("admin-628-user-sidebar.png"),
	});
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
	await beginAccess(page, personalRemark);
	await page.getByRole("button", { name: "List View", exact: true }).click();
	await expect(page.getByRole("cell", { name, exact: true })).toBeVisible();

	// Exercise the shared tree background, not the inner name button.
	const rootRow = page.locator("[data-folder-tree-root-row]");
	const rootBox = await rootRow.boundingBox();
	if (!rootBox) throw new Error("root tree row is not visible");
	await rootRow.click({ position: { x: rootBox.width - 3, y: 10 } });
	await expect(page).not.toHaveURL(/folderId=/);
	const treeRow = page.locator(`[data-folder-tree-row="${folder.id}"]`);
	await expect(treeRow).toBeVisible();
	const treeBox = await treeRow.boundingBox();
	if (!treeBox) throw new Error("folder tree row is not visible");
	await treeRow.click({ position: { x: treeBox.width - 3, y: 10 } });
	await expect(page).toHaveURL(new RegExp(`folderId=${folder.id}`));
	await treeRow
		.getByRole("button", { name: "Expand tree", exact: true })
		.click();
	await expect(page).toHaveURL(new RegExp(`folderId=${folder.id}`));
	await treeRow
		.getByRole("button", { name: "Collapse tree", exact: true })
		.click();
	await expect(page).toHaveURL(new RegExp(`folderId=${folder.id}`));
	const refresh = page.getByRole("button", { name: "Refresh", exact: true });
	const refreshBox = await refresh.boundingBox();
	const addressBox = await page
		.getByRole("navigation", { name: "breadcrumb" })
		.boundingBox();
	if (!refreshBox || !addressBox)
		throw new Error("file navigation controls are not visible");
	expect(refreshBox.x).toBeGreaterThan(addressBox.x + addressBox.width - 1);
	const targetImageRow = page.getByRole("row").filter({ hasText: imageName });
	await expect(targetImageRow.locator("img[src^='blob:']")).toBeVisible();
	await expect
		.poll(async () =>
			targetImageRow
				.locator("img")
				.evaluate((node: HTMLImageElement) => node.naturalWidth),
		)
		.toBeGreaterThan(0);
	await refresh.click();
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
		path: testInfo.outputPath("admin-628-workspace.png"),
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
	const infoPanel = page.getByRole("complementary", { name: "Details" });
	await expect(infoPanel).toBeVisible();
	await expect(infoPanel).toHaveClass(/opacity-100/);
	await expect(infoPanel.locator("..")).toHaveClass(/w-\[22rem\]/);
	await infoPanel.evaluate((node) => {
		node.setAttribute("data-retained-panel", "true");
	});

	await expect(page).toHaveURL(
		new RegExp(`/admin/users/${me.id}/files.*fileId=${file.id}`),
	);
	// Switching targets keeps the same expanded frame.
	await page.getByRole("cell", { name: imageName, exact: true }).click();
	await expect(
		infoPanel.getByRole("heading", { name: imageName, exact: true }),
	).toBeVisible();
	await expect(page.locator('[data-retained-panel="true"]')).toHaveClass(
		/opacity-100/,
	);
	await expect(infoPanel.locator("..")).toHaveClass(/w-\[22rem\]/);
	await page.getByRole("cell", { name, exact: true }).click();
	await expect(
		infoPanel.getByRole("heading", { name, exact: true }),
	).toBeVisible();
	await expect(page.locator('[data-retained-panel="true"]')).toHaveClass(
		/opacity-100/,
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
	await expect(page.locator('[data-retained-panel="true"]')).toHaveClass(
		/opacity-100/,
	);
	await expect(
		page.locator('[data-retained-panel="true"]').locator(".."),
	).toHaveClass(/w-\[22rem\]/);
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

	// The recycle bin spans all target directories, even when entered from a nested folder.
	const otherFolder = await createFolderViaApi(
		adminPage,
		"/api/v1",
		uniqueName("trash-location"),
	);
	const recycledName = `${uniqueName("recycled")}.txt`;
	const recycled = await uploadFileViaApi(
		adminPage,
		"/api/v1",
		{
			name: recycledName,
			mimeType: "text/plain",
			buffer: Buffer.from("recycle bytes"),
		},
		otherFolder.id,
	);
	await apiJsonInPage(adminPage, `/api/v1/files/${recycled.id}`, {
		method: "DELETE",
		withCsrf: true,
	});
	const exitSnapshot = await history.evaluate((node) => {
		const button = node.querySelector<HTMLButtonElement>(
			'[data-slot="dialog-close"]',
		);
		if (!button) throw new Error("version close button missing");
		button.click();
		return {
			title: node.querySelector("h2")?.textContent,
			rows: node.querySelectorAll("tbody tr").length,
		};
	});
	expect(exitSnapshot.title).toBe(`Version History - ${name}`);
	expect(exitSnapshot.rows).toBeGreaterThan(0);
	await expect(history).toBeHidden();
	await expect(page).toHaveURL(/panel=info/);
	await expect(infoPanel).toBeVisible();
	await expect(infoPanel).toHaveClass(/opacity-100/);
	await infoPanel.getByRole("button", { name: "Close", exact: true }).click();
	await expect(infoPanel).toHaveCount(0);
	// Open a folder through the shared item menu without changing the browsed directory.
	await page
		.locator("[data-folder-tree-root-row]")
		.getByRole("button", { name: "Root", exact: true })
		.click();
	await page
		.getByRole("row")
		.filter({ has: page.getByRole("cell", { name: folder.name, exact: true }) })
		.getByRole("button", { name: "More actions" })
		.click();
	await page.getByRole("menuitem", { name: "Details", exact: true }).click();
	await expect(page).toHaveURL(new RegExp(`infoFolderId=${folder.id}`));
	await expect(page).not.toHaveURL(/[?&]folderId=/);
	await expect(
		infoPanel.getByRole("heading", { name: folder.name, exact: true }),
	).toBeVisible();
	await expect(
		infoPanel.getByText("0 folder(s), 2 file(s)", { exact: true }).first(),
	).toBeVisible();
	await expect(
		infoPanel.getByRole("button", { name: /Versions|Download/ }),
	).toHaveCount(0);
	await page.screenshot({
		path: testInfo.outputPath("admin628-folder-details.png"),
		animations: "disabled",
	});
	await infoPanel.getByRole("button", { name: "Close", exact: true }).click();
	await expect(infoPanel).toHaveCount(0);
	await page.getByRole("button", { name: "Trash", exact: true }).click();
	await expect(page).toHaveURL(/view=trash/);
	await expect(
		page.getByRole("cell", { name: recycledName, exact: true }),
	).toBeVisible();
	await expect(
		page.getByRole("columnheader", { name: "Original location", exact: true }),
	).toBeVisible();
	await expect(
		page.getByRole("cell", { name: `/${otherFolder.name}`, exact: true }),
	).toBeVisible();
	await expect(
		page.getByRole("button", {
			name: /Restore|Permanently delete|Empty trash/,
		}),
	).toHaveCount(0);
	await expect(
		page.getByRole("button", { name: /Show deleted items in this folder/ }),
	).toHaveCount(0);
	expect(
		(
			await apiJsonInPage<{ total: number }>(
				page,
				"/api/v1/admin/audit-logs?action=admin_workspace_inspect",
			)
		).total,
	).toBe(inspectionCount);
	await page.screenshot({
		path: testInfo.outputPath("admin628-target-trash.png"),
		animations: "disabled",
	});

	// Blob observability hands off to the same logical revision URL.
	await page.goto(
		`/admin/file-blobs?hash=${encodeURIComponent(detail.blob.hash)}`,
	);
	await page
		.getByRole("row")
		.filter({ hasText: String(detail.blob.id) })
		.first()
		.click();
	await expect(page).toHaveURL(new RegExp(`blobId=${detail.blob.id}`));
	await page.reload();
	const blobDialog = page.getByRole("dialog");
	await expect(
		blobDialog.getByRole("heading", {
			name: `Blob #${detail.blob.id}`,
			exact: true,
		}),
	).toBeVisible();
	await page.screenshot({
		path: testInfo.outputPath("admin628-blob-uploader-horizontal.png"),
		animations: "disabled",
	});
	const reference = blobDialog.getByRole("link", {
		name: new RegExp(`v${revision.version}`),
	});
	const card = await reference.boundingBox();
	if (!card) throw new Error("version reference missing");
	await reference.click({
		position: { x: card.width - 4, y: card.height - 4 },
	});
	await expect(page).toHaveURL(
		new RegExp(`fileId=${file.id}.*revisionId=${revision.id}`),
	);
	const logicalDialog = page.getByRole("dialog");
	await expect(logicalDialog).toBeVisible();
	await expect(
		logicalDialog.getByRole("combobox", { name: "Access reason" }),
	).toHaveCount(0);
	const versionCard = logicalDialog.getByRole("link", {
		name: new RegExp(`^v${revision.version}`),
	});
	const versionBounds = await versionCard.boundingBox();
	if (!versionBounds) throw new Error("logical revision card missing");
	await versionCard.click({
		position: { x: versionBounds.width - 4, y: versionBounds.height - 4 },
	});
	const [directDownload] = await Promise.all([
		page.waitForEvent("download"),
		logicalDialog
			.getByRole("button", {
				name: `Download revision v${revision.version}`,
				exact: true,
			})
			.click(),
	]);
	expect(await readFile((await directDownload.path()) as string)).toEqual(
		original,
	);
	await page.reload();
	await expect(page.getByRole("dialog")).toBeVisible();
	const team = await createTeamViaApi(page, uniqueName("inspect-team"));
	await page.goto(`/admin/teams/${team.id}/files`);
	await beginAccess(page, teamRemark);
	await expect(
		page.getByText("No matching items in this folder", { exact: true }),
	).toBeVisible();
	await page.reload();
	await expect(
		page.getByRole("button", { name: "Begin read-only access" }),
	).toBeDisabled();
	await page.goto("/admin/audit?action=admin_workspace_inspect");
	// Audit rows sort newest first, and user/team IDs belong to independent sequences.
	// Select each exact access by its unique remark, then verify its target identity.
	const expectedAccesses = [
		{
			remark: personalRemark,
			target: `${me.username} (#${me.id})`,
			kind: "User",
		},
		{ remark: teamRemark, target: `${team.name} (#${team.id})`, kind: "Team" },
	];
	for (const access of expectedAccesses) {
		const row = page.getByRole("row").filter({ hasText: access.remark });
		await expect(row).toHaveCount(1);
		await expect(row).toContainText(access.target);
		await expect(
			row.getByRole("cell", { name: access.kind, exact: true }),
		).toBeVisible();
		await expect(row).toContainText("Reason User support");
		await expect(row).toContainText("Result Inspection authorized");
	}
	await page.screenshot({
		path: testInfo.outputPath("admin-628-audit-readable.png"),
	});
	await page.addInitScript(() => localStorage.setItem("aster-language", "zh"));
	await page.reload();
	for (const access of expectedAccesses) {
		const localized = page.getByRole("row").filter({ hasText: access.remark });
		await expect(localized).toHaveCount(1);
		await expect(localized).toContainText(access.target);
		await expect(localized).toContainText("原因 用户支持");
		await expect(localized).toContainText("已授权查看");
		await expect(localized).toContainText(`备注：${access.remark}`);
	}
	await page.screenshot({
		path: testInfo.outputPath("admin-628-audit-readable-zh.png"),
		animations: "disabled",
	});
});
