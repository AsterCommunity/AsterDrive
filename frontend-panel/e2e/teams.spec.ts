import type { UserInfo } from "@/types/api";
import { createTeamViaApi } from "./support/api";
import { authenticate } from "./support/auth";
import {
	clickMoreBatchAction,
	createFolderFromSurface,
	createPageShare,
	deleteItem,
	dialogByTitle,
	expectItemMissing,
	expectTrashItemMissing,
	expectTrashItemVisible,
	fileDropZone,
	fileNameCell,
	openFolder,
	openItemContextMenu,
	renameItem,
	toggleItemSelection,
	trashItemRow,
	uploadViaPicker,
} from "./support/files";
import { uniqueName } from "./support/fixtures";
import { apiJsonInPage } from "./support/network";
import { expectShareCardVisible } from "./support/shares";
import { expect, test } from "./support/test";

test.describe
	.serial("Team Workspace E2E", () => {
		test("uses team-scoped files, shares, search, trash, and tasks", async ({
			page,
			request,
		}) => {
			await authenticate(page, request);

			const teamName = uniqueName("pw-team");
			const team = await createTeamViaApi(
				page,
				teamName,
				"Team workspace created by Playwright",
			);
			const workspacePath = `/teams/${team.id}`;
			const file = {
				buffer: Buffer.from(
					"Team workspace coverage from Playwright\n",
					"utf8",
				),
				mimeType: "text/plain",
				name: `${uniqueName("pw-team-file")}.txt`,
			} as const;
			const folderName = uniqueName("pw-team-folder");
			const archiveNameStem = uniqueName("pw-team-bundle");
			const archiveName = `${archiveNameStem}.zip`;

			await page.goto(workspacePath);
			await expect(fileDropZone(page)).toBeVisible({ timeout: 30_000 });
			await expect(
				page.getByRole("button", {
					name: `Switch workspace, current: ${teamName}`,
				}),
			).toBeVisible({ timeout: 30_000 });

			await uploadViaPicker(page, [file]);
			await createFolderFromSurface(page, folderName);
			await expect(fileNameCell(page, file.name)).toBeVisible({
				timeout: 30_000,
			});
			await expect(fileNameCell(page, folderName)).toBeVisible({
				timeout: 30_000,
			});

			await page.getByRole("button", { name: "Open search" }).first().click();
			const searchDialog = page.getByRole("dialog");
			await expect(searchDialog).toBeVisible();
			await searchDialog
				.getByPlaceholder("Search files and folders...")
				.fill(file.name);
			await searchDialog
				.getByRole("button", { exact: true, name: "Search" })
				.click();
			await expect(searchDialog).toBeHidden();
			await expect(page).toHaveURL(new RegExp(`${workspacePath}/search\\?`));
			await expect(fileNameCell(page, file.name)).toBeVisible({
				timeout: 30_000,
			});
			await expect(fileNameCell(page, folderName)).toHaveCount(0);

			const shareUrl = await createPageShare(page, file.name);
			expect(shareUrl).toContain("/s/");
			await page.getByRole("link", { name: "My Shares" }).click();
			await expect(page).toHaveURL(new RegExp(`${workspacePath}/shares$`));
			await expectShareCardVisible(page, file.name);

			await page.goto(workspacePath);
			await expect(fileDropZone(page)).toBeVisible({ timeout: 30_000 });
			await expect(page).toHaveURL(new RegExp(`${workspacePath}$`));
			await openFolder(page, folderName);
			await expect(
				page
					.getByRole("navigation", { name: "Breadcrumb" })
					.getByText(folderName, { exact: true }),
			).toBeVisible();

			await page.goto(workspacePath);
			await expect(fileDropZone(page)).toBeVisible({ timeout: 30_000 });
			await deleteItem(page, folderName);
			await expectItemMissing(page, folderName);
			await page.getByRole("link", { name: "Trash" }).click();
			await expect(page).toHaveURL(new RegExp(`${workspacePath}/trash$`));
			await expectTrashItemVisible(page, folderName);
			await trashItemRow(page, folderName).click();
			await page.getByRole("button", { name: "Restore Selected" }).click();
			await expectTrashItemMissing(page, folderName);

			await page.goto(workspacePath);
			await expect(fileDropZone(page)).toBeVisible({ timeout: 30_000 });
			await expect(
				page.getByText(file.name, { exact: true }).first(),
			).toBeVisible({
				timeout: 30_000,
			});
			await toggleItemSelection(page, file.name);
			await clickMoreBatchAction(page, "Compress online");
			const archiveDialog = dialogByTitle(page, "Archive name");
			await expect(archiveDialog).toBeVisible();
			await archiveDialog
				.getByPlaceholder("Enter an archive name")
				.fill(archiveNameStem);
			await archiveDialog
				.getByRole("button", { name: "Create compression task" })
				.click();
			await expect(archiveDialog).toBeHidden();

			await page.getByRole("link", { name: "Tasks" }).click();
			await expect(page).toHaveURL(new RegExp(`${workspacePath}/tasks$`));
			const taskCard = page
				.locator('[data-slot="card"]')
				.filter({
					has: page.getByText(archiveName, { exact: true }),
					hasText: "1 items",
				})
				.first();
			await expect(taskCard).toBeVisible({
				timeout: 30_000,
			});
			await taskCard
				.getByRole("button", { exact: true, name: "Show details" })
				.click();
			await expect(taskCard.getByText("Prepare archive sources")).toBeVisible();
		});
	});

test("team file list and details retain creator and modifier after rename and refresh", async ({
	page,
	request,
}) => {
	await authenticate(page, request);
	const me = await apiJsonInPage<UserInfo>(page, "/api/v1/auth/me");
	const name = me.profile?.display_name || me.username;
	const team = await createTeamViaApi(
		page,
		uniqueName("pw-attribution"),
		"Attribution test",
	);
	const workspacePath = `/teams/${team.id}`;
	const file = {
		name: `${uniqueName("pw-authors")}.txt`,
		mimeType: "text/plain",
		buffer: Buffer.from("authors"),
	};
	await page.goto(workspacePath);
	await uploadViaPicker(page, [file]);
	const renamed = `${uniqueName("pw-authors-renamed")}.txt`;
	await renameItem(page, file.name, renamed);
	await page.reload();
	const listButton = page.getByRole("button", {
		name: "List View",
		exact: true,
	});
	if (await listButton.isVisible()) await listButton.click();
	const row = page.getByRole("row").filter({ hasText: renamed });
	await expect(row.getByText(name, { exact: true })).toHaveCount(2);
	await expect(
		page.getByRole("columnheader", { name: "Created by", exact: true }),
	).toBeVisible();
	await expect(
		page.getByRole("columnheader", { name: "Last modified by", exact: true }),
	).toBeVisible();
	await openItemContextMenu(page, renamed);
	await page.getByRole("menuitem", { name: "Details", exact: true }).click();
	const inspector = page.getByRole("complementary", { name: "Details" });
	await expect(
		inspector.getByText("Created by", { exact: true }),
	).toBeVisible();
	await expect(
		inspector.getByText("Last modified by", { exact: true }),
	).toBeVisible();
	await expect(
		inspector.locator("dd").getByText(name, { exact: true }),
	).toHaveCount(2);
	const activity = inspector.getByRole("region", { name: "File activity" });
	await expect(activity).toContainText(renamed);
	await expect(activity).toContainText("Renamed file");
});
