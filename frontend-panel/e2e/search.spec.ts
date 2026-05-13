import { authenticate } from "./support/auth";
import {
	createFolderFromSurface,
	fileNameCell,
	navigateToRoot,
	openFolder,
	uploadViaPicker,
} from "./support/files";
import { uniqueName } from "./support/fixtures";
import { expect, test } from "./support/test";

test.describe
	.serial("Search E2E", () => {
		test("searches files and folders in the current workspace", async ({
			page,
			request,
		}) => {
			await authenticate(page, request);

			const token = uniqueName("pw-search");
			const folderName = `${token}-folder`;
			const file = {
				buffer: Buffer.from("Searchable content from Playwright\n", "utf8"),
				mimeType: "text/plain",
				name: `${token}-note.txt`,
			} as const;

			await uploadViaPicker(page, [file]);
			await expect(fileNameCell(page, file.name)).toBeVisible({
				timeout: 30_000,
			});

			await createFolderFromSurface(page, folderName);

			await page.getByRole("button", { name: "Open search" }).first().click();
			const searchDialog = page.getByRole("dialog");
			await expect(searchDialog).toBeVisible();
			await searchDialog
				.getByPlaceholder("Search files and folders...")
				.fill(token);
			await expect(
				searchDialog.getByText(file.name, { exact: true }),
			).toBeVisible({
				timeout: 30_000,
			});
			await expect(
				searchDialog.getByText(folderName, { exact: true }),
			).toBeVisible();

			await searchDialog.getByRole("button", { name: "Files only" }).click();
			await expect(
				searchDialog.getByText(file.name, { exact: true }),
			).toBeVisible({
				timeout: 30_000,
			});
			await expect(
				searchDialog.getByText(folderName, { exact: true }),
			).toHaveCount(0);

			await searchDialog.getByRole("button", { name: "Folders only" }).click();
			await expect(
				searchDialog.getByText(folderName, { exact: true }),
			).toBeVisible({
				timeout: 30_000,
			});
			await expect(
				searchDialog.getByText(file.name, { exact: true }),
			).toHaveCount(0);

			await searchDialog.getByText(folderName, { exact: true }).click();
			await expect(searchDialog).toBeHidden();
			await expect(page).toHaveURL(/\/folder\/\d+/);
			await expect(
				page
					.getByRole("navigation", { name: "Breadcrumb" })
					.getByText(folderName, { exact: true }),
			).toBeVisible();

			await navigateToRoot(page);
			await openFolder(page, folderName);
		});
	});
