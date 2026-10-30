import { expect, type Page } from "@playwright/test";
import type { InitUploadResponse, ResourceLockState } from "@/types/api";
import { type E2eApiResponse, expectApiSuccess } from "./api-response";
import type { TestFile } from "./fixtures";
import { apiJsonInPage } from "./network";

export interface E2eFileListItem {
	id: number;
	name: string;
	size: number;
	mime_type: string;
	updated_at: string;
	lock_state: ResourceLockState;
	is_shared: boolean;
}

export interface E2eFolderListItem {
	id: number;
	name: string;
	updated_at: string;
	lock_state: ResourceLockState;
	is_shared: boolean;
}

export interface E2eFileInfo {
	id: number;
	name: string;
	folder_id?: number | null;
	team_id?: number | null;
	size: number;
	mime_type: string;
	updated_at: string;
	lock_state: ResourceLockState;
}

export interface E2eFolderInfo {
	id: number;
	name: string;
	parent_id?: number | null;
	team_id?: number | null;
	updated_at: string;
	lock_state: ResourceLockState;
}

export interface E2eFolderContents {
	files: E2eFileListItem[];
	files_total: number;
	folders: E2eFolderListItem[];
	folders_total: number;
	next_file_cursor?: { id: number; value: string } | null;
}

export interface E2eTaskInfo {
	id: number;
	kind: string;
	status: string;
	display_name: string;
	progress_percent: number;
	result?: unknown;
	steps: unknown[];
}

export interface E2eOffsetPage<T> {
	items: T[];
	limit: number;
	offset: number;
	total: number;
}

export interface E2eTeamInfo {
	id: number;
	name: string;
	description?: string | null;
	my_role?: "owner" | "admin" | "member";
	member_count?: number;
	storage_used?: number;
	storage_quota?: number;
	created_by_username?: string;
	created_at?: string;
	updated_at?: string;
	archived_at?: string | null;
}

export async function createTeamViaApi(
	page: Page,
	name: string,
	description?: string,
) {
	return apiJsonInPage<E2eTeamInfo>(page, "/api/v1/teams", {
		body: {
			description,
			name,
		},
		method: "POST",
		withCsrf: true,
	});
}

export async function createFolderViaApi(
	page: Page,
	workspacePath: string,
	name: string,
	parentId: number | null = null,
) {
	return apiJsonInPage<E2eFolderInfo>(page, `${workspacePath}/folders`, {
		body: {
			name,
			parent_id: parentId,
		},
		method: "POST",
		withCsrf: true,
	});
}

export async function listRootViaApi(page: Page, workspacePath: string) {
	return apiJsonInPage<E2eFolderContents>(page, `${workspacePath}/folders`);
}

export async function getFileIdByName(
	page: Page,
	workspacePath: string,
	fileName: string,
) {
	const contents = await listRootViaApi(page, workspacePath);
	const file = contents.files.find((item) => item.name === fileName);
	expect(
		file,
		`Expected file "${fileName}" to exist in ${workspacePath}`,
	).toBeTruthy();
	return file?.id ?? 0;
}

export async function uploadFileViaApi(
	page: Page,
	workspacePath: string,
	file: TestFile,
	folderId: number | null = null,
) {
	const response = await page.evaluate(
		async ({ bufferBase64, file, folderId, requestPath }) => {
			const readCookie = (name: string) => {
				const encodedName = `${encodeURIComponent(name)}=`;
				for (const chunk of document.cookie.split(";")) {
					const trimmed = chunk.trim();
					if (trimmed.startsWith(encodedName)) {
						return decodeURIComponent(trimmed.slice(encodedName.length));
					}
				}
				return null;
			};
			const binary = atob(bufferBase64);
			const bytes = Uint8Array.from(binary, (char) => char.charCodeAt(0));

			const csrfToken = readCookie("aster_csrf");
			const headers: Record<string, string> = csrfToken
				? { "X-CSRF-Token": csrfToken }
				: {};
			const initialized = await fetch(`${requestPath}/init`, {
				method: "POST",
				credentials: "include",
				headers: { ...headers, "Content-Type": "application/json" },
				body: JSON.stringify({
					filename: file.name,
					mime_type: file.mimeType,
					total_size: bytes.length,
					folder_id: folderId,
				}),
			});
			if (!initialized.ok)
				return { status: initialized.status, text: await initialized.text() };
			const payload =
				(await initialized.json()) as E2eApiResponse<InitUploadResponse>;
			if (!payload.data?.upload_id)
				throw new Error("upload init returned no session");
			const init = payload.data;
			let result: Response;
			if (init.mode === "stream") {
				result = await fetch(`${requestPath}/${init.upload_id}/body`, {
					method: "PUT",
					credentials: "include",
					headers: { ...headers, "Content-Type": "application/octet-stream" },
					body: bytes,
				});
			} else if (init.mode === "chunked") {
				if (!init.chunk_size || init.chunk_size <= 0)
					throw new Error("upload init returned invalid chunk size");
				for (
					let offset = 0, index = 0;
					offset < bytes.length;
					offset += init.chunk_size, index++
				) {
					const chunk = await fetch(
						`${requestPath}/${init.upload_id}/${index}`,
						{
							method: "PUT",
							credentials: "include",
							headers: {
								...headers,
								"Content-Type": "application/octet-stream",
							},
							body: bytes.slice(offset, offset + init.chunk_size),
						},
					);
					if (!chunk.ok)
						return { status: chunk.status, text: await chunk.text() };
				}
				result = await fetch(`${requestPath}/${init.upload_id}/complete`, {
					method: "POST",
					credentials: "include",
					headers,
				});
			} else {
				throw new Error(
					`server upload fixture requires stream or chunked mode, got ${init.mode}`,
				);
			}

			return {
				status: result.status,
				text: await result.text(),
			};
		},
		{
			bufferBase64: file.buffer.toString("base64"),
			file: {
				mimeType: file.mimeType,
				name: file.name,
			},
			folderId,
			requestPath: `${workspacePath}/files/upload`,
		},
	);

	expect(response.status).toBeGreaterThanOrEqual(200);
	expect(response.status).toBeLessThan(300);
	const payload = JSON.parse(response.text) as E2eApiResponse<E2eFileInfo>;
	expectApiSuccess(payload);
	return payload.data;
}
