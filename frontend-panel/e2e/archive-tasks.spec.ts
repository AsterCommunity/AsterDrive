import type { E2eOffsetPage, E2eTaskInfo } from "./support/api";
import { authenticate, gotoAdminPage } from "./support/auth";
import {
	dialogByTitle,
	fileNameCell,
	openItemContextMenu,
} from "./support/files";
import { uniqueName } from "./support/fixtures";
import { waitForApiCondition } from "./support/network";
import { expect, test } from "./support/test";

const ZIP_WITH_TEXT_FILE_BASE64 =
	"UEsDBBQACAAIABx+qVwAAAAAAAAAACIAAAAKABwAaW5zaWRlLnR4dFVUCQAD6Ob+aejm/ml1eAsAAQT2AQAABBQAAABzrSgpSkwuSU1RSM7PK0nNK1FIK8rPVQjISawsL8pMzyjhAgBQSwcIUTpVEiQAAAAiAAAAUEsBAh4DFAAIAAgAHH6pXFE6VRIkAAAAIgAAAAoAGAAAAAAAAQAAAKSBAAAAAGluc2lkZS50eHRVVAUAA+jm/ml1eAsAAQT2AQAABBQAAABQSwUGAAAAAAEAAQBQAAAAeAAAAAAA";

test.describe
	.serial("Archive Task E2E", () => {
		test("extracts an uploaded archive and exposes the task in user and admin views", async ({
			page,
			request,
		}) => {
			await authenticate(page, request);

			const archiveFile = {
				buffer: Buffer.from(ZIP_WITH_TEXT_FILE_BASE64, "base64"),
				mimeType: "application/zip",
				name: `${uniqueName("pw-extract-source")}.zip`,
			} as const;
			const outputFolderName = uniqueName("pw-extracted");

			await page.goto("/");
			await page.getByTestId("upload-file-input").setInputFiles({
				buffer: archiveFile.buffer,
				mimeType: archiveFile.mimeType,
				name: archiveFile.name,
			});
			await expect(fileNameCell(page, archiveFile.name)).toBeVisible({
				timeout: 30_000,
			});

			await openItemContextMenu(page, archiveFile.name);
			await page
				.getByRole("menuitem", { exact: true, name: "Extract online" })
				.click();

			const extractDialog = dialogByTitle(page, "Output folder name");
			await expect(extractDialog).toBeVisible();
			await extractDialog
				.getByPlaceholder("Enter the extracted folder name")
				.fill(outputFolderName);
			await extractDialog
				.getByRole("button", { name: "Create extract task" })
				.click();
			await expect(extractDialog).toBeHidden();

			const displayName = `Extract ${archiveFile.name}`;
			await waitForApiCondition<E2eOffsetPage<E2eTaskInfo>>(
				page,
				"/api/v1/tasks?limit=20&offset=0",
				(data) =>
					data.items.some(
						(task) =>
							task.display_name === displayName && task.status === "succeeded",
					),
				{ timeoutMs: 60_000 },
			);

			await page.getByRole("link", { name: "Tasks" }).click();
			await expect(page).toHaveURL(/\/tasks$/);
			await expect(page.getByText(displayName, { exact: true })).toBeVisible({
				timeout: 30_000,
			});
			await page.getByRole("button", { name: "Show details" }).first().click();
			await expect(page.getByText("Extract to staging")).toBeVisible();
			await expect(page.getByText("Import to workspace")).toBeVisible();
			await page
				.getByRole("button", { exact: true, name: "Open target folder" })
				.click();
			await expect(
				page
					.getByRole("navigation", { name: "Breadcrumb" })
					.getByText(outputFolderName, { exact: true }),
			).toBeVisible({ timeout: 30_000 });
			await expect(fileNameCell(page, "inside.txt")).toBeVisible({
				timeout: 30_000,
			});

			await gotoAdminPage(
				page,
				"/admin/tasks?kind=archive_extract&status=succeeded",
				"Tasks",
			);
			await expect(page.getByText(displayName, { exact: true })).toBeVisible({
				timeout: 30_000,
			});
			await expect(page.getByText("Archive extraction").first()).toBeVisible();
			await expect(page.getByText("Completed").first()).toBeVisible();
		});
	});
