import { createTeamViaApi } from "./support/api";
import { authenticate } from "./support/auth";
import {
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
	toggleItemSelection,
	trashItemRow,
	uploadViaPicker,
} from "./support/files";
import { uniqueName } from "./support/fixtures";
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
			const archiveName = `${uniqueName("pw-team-bundle")}.zip`;

			await page.goto(workspacePath);
			await expect(fileDropZone(page)).toBeVisible({ timeout: 30_000 });
			await expect(page.getByRole("link", { name: teamName })).toBeVisible({
				timeout: 30_000,
			});

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
			await expect(
				searchDialog.getByText(file.name, { exact: true }),
			).toBeVisible({
				timeout: 30_000,
			});
			await expect(
				searchDialog.getByText(folderName, { exact: true }),
			).toHaveCount(0);
			await page.keyboard.press("Escape");
			await expect(searchDialog).toBeHidden();

			const shareUrl = await createPageShare(page, file.name);
			expect(shareUrl).toContain("/s/");
			await page.getByRole("link", { name: "My Shares" }).click();
			await expect(page).toHaveURL(new RegExp(`${workspacePath}/shares$`));
			await expect(page.getByText(file.name, { exact: true })).toBeVisible({
				timeout: 30_000,
			});

			await page.getByRole("link", { name: teamName }).click();
			await expect(page).toHaveURL(new RegExp(`${workspacePath}$`));
			await openFolder(page, folderName);
			await expect(
				page
					.getByRole("navigation", { name: "Breadcrumb" })
					.getByText(folderName, { exact: true }),
			).toBeVisible();

			await page.getByRole("link", { name: teamName }).click();
			await deleteItem(page, folderName);
			await expectItemMissing(page, folderName);
			await page.getByRole("link", { name: "Trash" }).click();
			await expect(page).toHaveURL(new RegExp(`${workspacePath}/trash$`));
			await expectTrashItemVisible(page, folderName);
			await trashItemRow(page, folderName).click();
			await page.getByRole("button", { name: "Restore Selected" }).click();
			await expectTrashItemMissing(page, folderName);

			await page.getByRole("link", { name: teamName }).click();
			await expect(
				page.getByText(file.name, { exact: true }).first(),
			).toBeVisible({
				timeout: 30_000,
			});
			await toggleItemSelection(page, file.name);
			await page
				.getByRole("button", { exact: true, name: "Compress online" })
				.click();
			const archiveDialog = dialogByTitle(page, "Archive name");
			await expect(archiveDialog).toBeVisible();
			await archiveDialog
				.getByPlaceholder("Enter an archive name")
				.fill(archiveName);
			await archiveDialog
				.getByRole("button", { name: "Create compression task" })
				.click();
			await expect(archiveDialog).toBeHidden();

			await page.getByRole("link", { name: "Tasks" }).click();
			await expect(page).toHaveURL(new RegExp(`${workspacePath}/tasks$`));
			await expect(
				page.getByText(`Compress ${archiveName}`, { exact: true }),
			).toBeVisible({
				timeout: 30_000,
			});
			await page.getByRole("button", { name: "Show details" }).first().click();
			await expect(page.getByText("Prepare archive sources")).toBeVisible();
		});
	});
