import { readFile } from "node:fs/promises";
import path from "node:path";
import { expect, type Locator, type Page } from "@playwright/test";
import type { TestFile } from "./fixtures";

export function fileDropZone(page: Page) {
	return page.locator('[aria-label="File drop zone"]').first();
}

export function fileNameCell(page: Page, fileName: string) {
	return page.getByRole("cell", { exact: true, name: fileName }).first();
}

export function tableRowByCellText(page: Page, cellText: string) {
	return page
		.getByRole("row")
		.filter({ has: page.getByRole("cell", { exact: true, name: cellText }) })
		.first();
}

export function fileRow(page: Page, fileName: string) {
	return tableRowByCellText(page, fileName);
}

export function dialogByTitle(page: Page, title: string) {
	return page
		.getByRole("dialog")
		.filter({ has: page.getByRole("heading", { exact: true, name: title }) })
		.first();
}

export async function uploadViaPicker(
	page: Page,
	files: ReadonlyArray<TestFile>,
) {
	await page.getByTestId("upload-file-input").setInputFiles(
		files.map((file) => ({
			buffer: file.buffer,
			mimeType: file.mimeType,
			name: file.name,
		})),
	);
}

export async function uploadViaDragDrop(
	page: Page,
	files: ReadonlyArray<TestFile>,
) {
	const dataTransfer = await page.evaluateHandle(
		(payload) => {
			const transfer = new DataTransfer();

			for (const file of payload) {
				const binary = atob(file.bufferBase64);
				const bytes = Uint8Array.from(binary, (char) => char.charCodeAt(0));
				transfer.items.add(
					new File([bytes], file.name, { type: file.mimeType }),
				);
			}

			return transfer;
		},
		files.map((file) => ({
			bufferBase64: file.buffer.toString("base64"),
			mimeType: file.mimeType,
			name: file.name,
		})),
	);

	const dropZone = fileDropZone(page);
	await dropZone.dispatchEvent("dragenter", { dataTransfer });
	await dropZone.dispatchEvent("dragover", { dataTransfer });
	await dropZone.dispatchEvent("drop", { dataTransfer });
}

export async function openSurfaceContextMenu(page: Page) {
	const trigger = fileDropZone(page)
		.locator('[data-slot="context-menu-trigger"]')
		.first();
	await trigger.evaluate((element) => {
		const rect = element.getBoundingClientRect();
		element.dispatchEvent(
			new MouseEvent("contextmenu", {
				bubbles: true,
				button: 2,
				cancelable: true,
				clientX: rect.left + Math.max(24, rect.width - 80),
				clientY: rect.top + Math.max(24, rect.height - 80),
				view: window,
			}),
		);
	});
}

export async function createFolderFromSurface(page: Page, folderName: string) {
	await openSurfaceContextMenu(page);
	await page.getByRole("menuitem", { name: "New Folder" }).click();

	const dialog = page.getByRole("dialog");
	await expect(dialog).toBeVisible();
	await dialog.getByPlaceholder("Folder name").fill(folderName);
	await dialog.getByRole("button", { name: "Create Folder" }).click();
	await expect(dialog).toBeHidden();
	await expect(fileNameCell(page, folderName)).toBeVisible({ timeout: 30_000 });
}

export async function expectImagePreview(page: Page, fileName: string) {
	await fileNameCell(page, fileName).click();
	await chooseOpenMethodIfPrompted(page, "Image preview");
	const dialog = page.getByRole("dialog");
	await expect(dialog).toBeVisible();
	await expect(dialog.getByRole("img", { name: fileName })).toBeVisible({
		timeout: 30_000,
	});
}

export async function expectPdfPreview(page: Page, fileName: string) {
	await fileNameCell(page, fileName).click();
	await chooseOpenMethodIfPrompted(page, "PDF preview");
	const dialog = page.getByRole("dialog");
	await expect(dialog).toBeVisible();
	await expect(dialog.locator("canvas").first()).toBeVisible({
		timeout: 30_000,
	});
}

export async function expectCodePreview(page: Page, fileName: string) {
	await fileNameCell(page, fileName).click();
	await chooseOpenMethodIfPrompted(page, "Source view");
	const dialog = page.getByRole("dialog");
	await expect(dialog).toBeVisible();
	await expect(
		dialog.getByText('const greeting = "Hello from Playwright";'),
	).toBeVisible({
		timeout: 30_000,
	});
}

export async function closeActiveDialog(page: Page) {
	const dialog = page.getByRole("dialog");
	const closeButton = dialog.getByRole("button", {
		exact: true,
		name: "Close",
	});
	if (await closeButton.isVisible().catch(() => false)) {
		await closeButton.click();
	} else {
		await page.keyboard.press("Escape");
	}
	await expect(dialog).toBeHidden();
}

export async function chooseOpenMethodIfPrompted(
	page: Page,
	label: string,
	timeoutMs = 0,
) {
	const chooser = page.getByRole("dialog", {
		name: "Choose how to open this file",
	});
	const chooserVisible =
		timeoutMs > 0
			? await chooser
					.waitFor({
						state: "visible",
						timeout: timeoutMs,
					})
					.then(() => true)
					.catch(() => false)
			: await chooser.isVisible().catch(() => false);

	if (!chooserVisible) {
		return;
	}

	let option = chooser.getByRole("button", { exact: true, name: label });
	if (!(await option.isVisible().catch(() => false))) {
		const more = chooser.getByRole("button", {
			exact: true,
			name: "More open methods",
		});
		if (await more.isVisible().catch(() => false)) {
			await more.click();
		}
		option = chooser.getByRole("button", { exact: true, name: label });
	}

	await option.click();
}

export async function openItemContextMenu(page: Page, itemName: string) {
	await fileRow(page, itemName).click({ button: "right" });
}

export async function renameItem(
	page: Page,
	currentName: string,
	nextName: string,
) {
	await openItemContextMenu(page, currentName);
	await page.getByRole("menuitem", { name: "Rename" }).click();

	const dialog = page.getByRole("dialog");
	await expect(dialog).toBeVisible();
	await dialog.getByRole("textbox").fill(nextName);
	await dialog.getByRole("button", { name: "Rename" }).click();
	await expect(dialog).toBeHidden();
}

export async function chooseTargetFolder(
	page: Page,
	targetFolderName: string,
	confirmLabel: "Copy here" | "Move here",
) {
	const dialog = page.getByRole("dialog");
	await expect(dialog).toBeVisible();
	await dialog
		.getByRole("button", { exact: true, name: targetFolderName })
		.click();
	await dialog.getByRole("button", { name: confirmLabel }).click();
	await expect(dialog).toBeHidden();
}

export async function copyItemToFolder(
	page: Page,
	itemName: string,
	targetFolderName: string,
) {
	await openItemContextMenu(page, itemName);
	await page.getByRole("menuitem", { name: "Copy" }).click();
	await chooseTargetFolder(page, targetFolderName, "Copy here");
}

export async function moveItemToFolder(
	page: Page,
	itemName: string,
	targetFolderName: string,
) {
	await openItemContextMenu(page, itemName);
	await page.getByRole("menuitem", { name: "Move" }).click();
	await chooseTargetFolder(page, targetFolderName, "Move here");
}

export async function deleteItem(page: Page, itemName: string) {
	await openItemContextMenu(page, itemName);
	await page.getByRole("menuitem", { name: "Delete" }).click();
}

export async function toggleItemSelection(page: Page, itemName: string) {
	await page
		.getByRole("row")
		.filter({ has: page.getByText(itemName, { exact: true }) })
		.first()
		.locator("button[aria-pressed]")
		.first()
		.click();
}

export async function clickMoreBatchAction(page: Page, actionName: string) {
	await page.getByRole("button", { name: "More batch actions" }).click();
	await page.getByRole("menuitem", { exact: true, name: actionName }).click();
}

export function folderTreeButton(page: Page, folderName: string) {
	return page
		.getByRole("complementary")
		.getByRole("button", { exact: true, name: folderName })
		.first();
}

export async function openFolder(page: Page, folderName: string) {
	await expect(folderTreeButton(page, folderName)).toBeVisible({
		timeout: 30_000,
	});
	await folderTreeButton(page, folderName).click();
	await expect(
		page.getByRole("navigation", { name: "Breadcrumb" }).getByText(folderName, {
			exact: true,
		}),
	).toBeVisible();
}

export async function navigateToRoot(page: Page) {
	await page
		.getByRole("complementary")
		.getByRole("button", { exact: true, name: "Root" })
		.click();
	await expect(fileDropZone(page)).toBeVisible();
	await expect(page).toHaveURL(/\/$/);
}

export async function expectItemMissing(page: Page, itemName: string) {
	await expect(fileNameCell(page, itemName)).toHaveCount(0, {
		timeout: 30_000,
	});
}

export function trashItemRow(page: Page, itemName: string) {
	return page
		.getByRole("row")
		.filter({ has: page.getByText(itemName, { exact: true }) })
		.first();
}

export async function expectTrashItemVisible(page: Page, itemName: string) {
	await expect(trashItemRow(page, itemName)).toBeVisible({ timeout: 30_000 });
}

export async function expectTrashItemMissing(page: Page, itemName: string) {
	await expect(trashItemRow(page, itemName)).toHaveCount(0, {
		timeout: 30_000,
	});
}

export async function expectDownloadMatches(
	page: Page,
	fileName: string,
	expected: Buffer,
	outputDir: string,
) {
	const downloadPromise = page.waitForEvent("download");
	await openItemContextMenu(page, fileName);
	await page.getByRole("menuitem", { name: "Download" }).click();

	const download = await downloadPromise;
	const targetPath = path.join(outputDir, download.suggestedFilename());
	await download.saveAs(targetPath);

	const actual = await readFile(targetPath);
	expect(actual.equals(expected)).toBe(true);
}

export async function createPageShare(
	page: Page,
	itemName: string,
	options?: {
		password?: string;
	},
) {
	await openItemContextMenu(page, itemName);
	await page.getByRole("menuitem", { exact: true, name: "Share" }).click();

	const dialog = page.getByRole("dialog");
	await expect(dialog).toBeVisible();
	if (options?.password) {
		await dialog.getByLabel("Password (optional)").fill(options.password);
	}
	await dialog
		.getByRole("button", { exact: true, name: "Create link" })
		.click();

	const shareUrlInput = dialog.getByTestId("share-primary-url");
	await expect(shareUrlInput).toHaveValue(/\/s\//);
	const shareUrl = await shareUrlInput.inputValue();

	await dialog.getByRole("button", { name: "Done" }).click();
	await expect(dialog).toBeHidden();

	return shareUrl;
}

export async function clickRowAction(row: Locator, title: string) {
	await row.getByTitle(title).click();
}
