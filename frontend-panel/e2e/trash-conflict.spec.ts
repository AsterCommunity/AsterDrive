import { createFolderViaApi } from "./support/api";
import { authenticate } from "./support/auth";
import {
	expectTrashItemMissing,
	expectTrashItemVisible,
	trashItemRow,
} from "./support/files";
import { uniqueName } from "./support/fixtures";
import { apiJsonInPage } from "./support/network";
import { expect, test } from "./support/test";

test("shows the localized purge conflict and allows retry after it clears", async ({
	page,
	request,
}) => {
	await authenticate(page, request);
	const name = uniqueName("trash-conflict");
	const folder = await createFolderViaApi(page, "/api/v1", name);
	await apiJsonInPage(page, `/api/v1/folders/${folder.id}`, {
		method: "DELETE",
		withCsrf: true,
	});
	await page.getByRole("link", { name: "Trash" }).click();
	await expectTrashItemVisible(page, name);
	// The API concurrency tests prove the real rejection. This browser boundary verifies
	// that the exact wire contract survives HTTP decoding, batching and localization.
	const route = `**/api/v1/trash/folder/${folder.id}/restore`;
	await page.route(route, async (requestRoute) => {
		await requestRoute.fulfill({
			status: 409,
			contentType: "application/json",
			body: JSON.stringify({
				code: "trash.purge_in_progress",
				msg: "Conflict",
				error: { retryable: false },
			}),
		});
	});
	await trashItemRow(page, name).click();
	await page.getByRole("button", { name: "Restore Selected" }).click();
	await expect(
		page.getByText(
			"Trash is being emptied. Wait for the task to finish before restoring, moving items to trash, or permanently deleting them",
			{ exact: false },
		),
	).toBeVisible();
	await expectTrashItemVisible(page, name);
	await page.unroute(route);
	await page.route(route, async (requestRoute) => {
		await requestRoute.fulfill({
			status: 202,
			contentType: "application/json",
			body: JSON.stringify({
				code: "success",
				msg: "",
				data: {
					id: 77,
					kind: "folder_tree_mutation",
					status: "pending",
					display_name: "Restore folder tree",
				},
			}),
		});
	});
	await page.getByRole("button", { name: "Restore Selected" }).click();
	await expect(
		page.getByText("Task added to the task center", { exact: true }),
	).toBeVisible();
	await expect(
		page.getByText("Restored 1 item(s)", { exact: true }),
	).toHaveCount(0);
	await expectTrashItemVisible(page, name);
	await page.unroute(route);
	await page.getByRole("button", { name: "Restore Selected" }).click();
	await expectTrashItemMissing(page, name);
});
