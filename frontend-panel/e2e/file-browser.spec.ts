import { authenticate, captureClientState } from "./support/auth";
import {
	chooseTargetFolder,
	clickMoreBatchAction,
	closeActiveDialog,
	copyItemToFolder,
	createFolderFromSurface,
	createPageShare,
	deleteItem,
	expectCodePreview,
	expectDownloadMatches,
	expectImagePreview,
	expectItemMissing,
	expectPdfPreview,
	expectTrashItemMissing,
	expectTrashItemVisible,
	fileNameCell,
	folderTreeButton,
	moveItemToFolder,
	navigateToRoot,
	openFolder,
	renameItem,
	toggleItemSelection,
	trashItemRow,
	uploadViaDragDrop,
	uploadViaPicker,
} from "./support/files";
import {
	CODE_FILE,
	IMAGE_FILE,
	PDF_FILE,
	uniqueName,
} from "./support/fixtures";
import {
	apiJsonInPage,
	loadPersistedSessions,
	saveResumableSession,
	uploadChunkViaApi,
} from "./support/network";
import { expectAnonymousSharePreview } from "./support/shares";
import { expect, test } from "./support/test";

test.describe
	.serial("File Browser E2E", () => {
		test("uploads, previews, downloads, and opens an anonymous share page", async ({
			browser,
			page,
			request,
		}, testInfo) => {
			await authenticate(page, request);

			await uploadViaPicker(page, [IMAGE_FILE, PDF_FILE]);
			await uploadViaDragDrop(page, [CODE_FILE]);

			for (const fileName of [IMAGE_FILE.name, PDF_FILE.name, CODE_FILE.name]) {
				await expect(fileNameCell(page, fileName)).toBeVisible({
					timeout: 30_000,
				});
			}

			await expectImagePreview(page, IMAGE_FILE.name);
			await closeActiveDialog(page);

			await expectPdfPreview(page, PDF_FILE.name);
			await closeActiveDialog(page);

			await expectCodePreview(page, CODE_FILE.name);
			await closeActiveDialog(page);

			await expectDownloadMatches(
				page,
				CODE_FILE.name,
				CODE_FILE.buffer,
				testInfo.outputDir,
			);

			const shareUrl = await createPageShare(page, IMAGE_FILE.name);
			const clientState = await captureClientState(page);
			await expectAnonymousSharePreview(
				browser,
				shareUrl,
				IMAGE_FILE.name,
				clientState,
			);
		});

		test("manages folders, files, and trash lifecycle flows", async ({
			page,
			request,
		}) => {
			await authenticate(page, request);

			const projectFolder = "pw-life-projects";
			const archiveFolder = "pw-life-archive";
			const referencesFolder = "pw-life-references";
			const lifecycleFile = {
				buffer: Buffer.from("Lifecycle flow from Playwright\n", "utf8"),
				mimeType: "text/plain",
				name: "pw-life-note.txt",
			} as const;
			const renamedLifecycleFile = "pw-life-note-renamed.txt";

			await createFolderFromSurface(page, projectFolder);
			await createFolderFromSurface(page, archiveFolder);

			await uploadViaPicker(page, [lifecycleFile]);
			await expect(fileNameCell(page, lifecycleFile.name)).toBeVisible({
				timeout: 30_000,
			});

			await renameItem(page, lifecycleFile.name, renamedLifecycleFile);
			await expect(fileNameCell(page, renamedLifecycleFile)).toBeVisible({
				timeout: 30_000,
			});

			await copyItemToFolder(page, renamedLifecycleFile, archiveFolder);
			await openFolder(page, archiveFolder);
			await expect(fileNameCell(page, renamedLifecycleFile)).toBeVisible({
				timeout: 30_000,
			});

			await navigateToRoot(page);
			await renameItem(page, archiveFolder, referencesFolder);
			await expect(fileNameCell(page, referencesFolder)).toBeVisible({
				timeout: 30_000,
			});

			await moveItemToFolder(page, renamedLifecycleFile, projectFolder);
			await expectItemMissing(page, renamedLifecycleFile);

			await openFolder(page, projectFolder);
			await expect(fileNameCell(page, renamedLifecycleFile)).toBeVisible({
				timeout: 30_000,
			});

			await navigateToRoot(page);
			await deleteItem(page, projectFolder);
			await deleteItem(page, referencesFolder);
			await expectItemMissing(page, projectFolder);
			await expectItemMissing(page, referencesFolder);

			await page.getByRole("link", { name: "Trash" }).click();
			await expect(page).toHaveURL(/\/trash$/);
			await expectTrashItemVisible(page, projectFolder);
			await expectTrashItemVisible(page, referencesFolder);

			await trashItemRow(page, projectFolder).click();
			await page.getByRole("button", { name: "Restore Selected" }).click();
			await expectTrashItemMissing(page, projectFolder);

			await trashItemRow(page, referencesFolder).click();
			await page
				.getByRole("button", { name: "Delete Selected Permanently" })
				.click();
			await page.getByRole("button", { name: "Permanently Delete" }).click();
			await expectTrashItemMissing(page, referencesFolder);

			await navigateToRoot(page);
			await expect(folderTreeButton(page, projectFolder)).toBeVisible({
				timeout: 30_000,
			});
			await expect(folderTreeButton(page, referencesFolder)).toHaveCount(0);

			await openFolder(page, projectFolder);
			await expect(fileNameCell(page, renamedLifecycleFile)).toBeVisible({
				timeout: 30_000,
			});
		});

		test("applies batch copy, move, and delete operations from multi-selection", async ({
			page,
			request,
		}) => {
			await authenticate(page, request);

			const copyTarget = uniqueName("pw-batch-copy");
			const moveTarget = uniqueName("pw-batch-move");
			const firstFile = {
				buffer: Buffer.from("batch file alpha\n", "utf8"),
				mimeType: "text/plain",
				name: `${uniqueName("pw-batch-alpha")}.txt`,
			} as const;
			const secondFile = {
				buffer: Buffer.from("batch file beta\n", "utf8"),
				mimeType: "text/plain",
				name: `${uniqueName("pw-batch-beta")}.txt`,
			} as const;

			await createFolderFromSurface(page, copyTarget);
			await createFolderFromSurface(page, moveTarget);
			await uploadViaPicker(page, [firstFile, secondFile]);
			await expect(fileNameCell(page, firstFile.name)).toBeVisible({
				timeout: 30_000,
			});
			await expect(fileNameCell(page, secondFile.name)).toBeVisible({
				timeout: 30_000,
			});

			await toggleItemSelection(page, firstFile.name);
			await toggleItemSelection(page, secondFile.name);
			await expect(page.getByText("2 selected")).toBeVisible();
			await page.getByRole("button", { exact: true, name: "Copy" }).click();
			await chooseTargetFolder(page, copyTarget, "Copy here");

			await openFolder(page, copyTarget);
			await expect(fileNameCell(page, firstFile.name)).toBeVisible({
				timeout: 30_000,
			});
			await expect(fileNameCell(page, secondFile.name)).toBeVisible({
				timeout: 30_000,
			});

			await navigateToRoot(page);
			await toggleItemSelection(page, firstFile.name);
			await toggleItemSelection(page, secondFile.name);
			await page.getByRole("button", { exact: true, name: "Move" }).click();
			await chooseTargetFolder(page, moveTarget, "Move here");
			await expectItemMissing(page, firstFile.name);
			await expectItemMissing(page, secondFile.name);

			await openFolder(page, moveTarget);
			await expect(fileNameCell(page, firstFile.name)).toBeVisible({
				timeout: 30_000,
			});
			await expect(fileNameCell(page, secondFile.name)).toBeVisible({
				timeout: 30_000,
			});

			await toggleItemSelection(page, firstFile.name);
			await toggleItemSelection(page, secondFile.name);
			await clickMoreBatchAction(page, "Delete");
			const deleteDialog = page.getByRole("alertdialog");
			await expect(deleteDialog).toBeVisible();
			await deleteDialog.getByRole("button", { name: "Delete" }).click();
			await expectItemMissing(page, firstFile.name);
			await expectItemMissing(page, secondFile.name);

			await page.getByRole("link", { name: "Trash" }).click();
			await expect(page).toHaveURL(/\/trash$/);
			await expectTrashItemVisible(page, firstFile.name);
			await expectTrashItemVisible(page, secondFile.name);
		});

		test("resumes a chunked upload from persisted progress", async ({
			page,
			request,
		}) => {
			await authenticate(page, request);

			const filename = `${uniqueName("pw-resume")}.bin`;
			const buffer = Buffer.alloc(6 * 1024 * 1024 + 257, 0x61);
			const init = await apiJsonInPage<{
				chunk_size: number;
				mode?: string;
				total_chunks: number;
				upload_id: string;
			}>(page, "/api/v1/files/upload/init", {
				method: "POST",
				body: {
					filename,
					total_size: buffer.length,
				},
				withCsrf: true,
			});
			expect(init.total_chunks).toBeGreaterThan(1);
			expect(init.chunk_size).toBeGreaterThan(0);

			await uploadChunkViaApi(
				page,
				init.upload_id,
				0,
				buffer.subarray(0, init.chunk_size),
			);

			const progress = await apiJsonInPage<{
				received_count: number;
				status: string;
			}>(page, `/api/v1/files/upload/${init.upload_id}`);
			expect(progress.received_count).toBe(1);
			expect(progress.status).toBe("uploading");

			await saveResumableSession(page, {
				baseFolderId: null,
				baseFolderName: "My Drive",
				chunkSize: init.chunk_size,
				filename,
				mode: "chunked",
				relativePath: null,
				savedAt: Date.now(),
				totalChunks: init.total_chunks,
				totalSize: buffer.length,
				uploadId: init.upload_id,
				workspace: { kind: "personal" },
			});

			await page.reload();
			await expect(page.getByText(filename, { exact: true })).toBeVisible({
				timeout: 30_000,
			});
			await expect(page.getByText("Chunked", { exact: true })).toBeVisible();
			await expect(
				page.getByText(`Chunk 1/${init.total_chunks}`, { exact: true }),
			).toBeVisible();
			await page.getByTitle("Select file to resume").first().click();
			await page.getByTestId("resume-input").setInputFiles({
				buffer,
				mimeType: "application/octet-stream",
				name: filename,
			});

			await expect(fileNameCell(page, filename)).toBeVisible({
				timeout: 30_000,
			});
			expect(await loadPersistedSessions(page)).toHaveLength(0);
		});
	});
