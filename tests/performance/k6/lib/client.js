import { check, fail } from "k6";
import encoding from "k6/encoding";
import exec from "k6/execution";
import http from "k6/http";

import { benchConfig } from "./config.js";

function url(path) {
	return `${benchConfig.baseUrl}${path}`;
}

function truncate(body) {
	if (!body) {
		return "";
	}

	return body.length > 240 ? `${body.slice(0, 240)}...` : body;
}

function buildQuery(params = {}) {
	const entries = Object.entries(params).filter(
		([, value]) => value !== undefined && value !== null && value !== "",
	);
	if (entries.length === 0) {
		return "";
	}

	const query = entries
		.map(
			([key, value]) =>
				`${encodeURIComponent(key)}=${encodeURIComponent(String(value))}`,
		)
		.join("&");
	return `?${query}`;
}

function cookieValue(response, name) {
	const cookie = response.cookies[name];
	return cookie && cookie.length > 0 ? cookie[0].value : null;
}

function parseApiBody(response) {
	try {
		return response.json();
	} catch {
		return null;
	}
}

function isSuccessCode(code) {
	return code === "success" || code === 0 || code === "0";
}

export function assertApi(response, context, expectedStatus = 200) {
	const body = parseApiBody(response);
	const success = check(response, {
		[`${context}: status ${expectedStatus}`]: (resp) =>
			resp.status === expectedStatus,
		[`${context}: api code 0`]: () => body !== null && isSuccessCode(body.code),
	});

	if (!success) {
		fail(
			`${context} failed with ${response.status}: ${truncate(response.body)}`,
		);
	}

	return body;
}

export function authHeaders(session, extra = {}) {
	return {
		Authorization: `Bearer ${session.accessToken}`,
		Cookie: `aster_access=${session.accessToken}; aster_csrf=${session.csrfToken}`,
		"X-CSRF-Token": session.csrfToken,
		...extra,
	};
}

export function login(
	username = benchConfig.username,
	password = benchConfig.password,
) {
	const response = http.post(
		url("/api/v1/auth/login"),
		JSON.stringify({
			identifier: username,
			password,
		}),
		{
			headers: {
				"Content-Type": "application/json",
			},
		},
	);
	const body = assertApi(response, "auth.login", 200);
	const accessToken = cookieValue(response, "aster_access");
	const refreshToken = cookieValue(response, "aster_refresh");
	const csrfToken = cookieValue(response, "aster_csrf");
	if (!accessToken || !refreshToken || !csrfToken) {
		fail("auth.login did not return required cookies");
	}

	const expiresIn = Number(body?.data?.expires_in ?? 0);
	return {
		accessToken,
		refreshToken,
		csrfToken,
		expiresAt: Date.now() + expiresIn * 1000,
		expiresIn,
		lastDuration: response.timings.duration,
	};
}

export function refreshSession(session) {
	const response = http.post(url("/api/v1/auth/refresh"), null, {
		headers: {
			Cookie: `aster_refresh=${session.refreshToken}; aster_csrf=${session.csrfToken}`,
			"X-CSRF-Token": session.csrfToken,
		},
	});
	const body = assertApi(response, "auth.refresh", 200);
	const accessToken = cookieValue(response, "aster_access");
	const refreshToken = cookieValue(response, "aster_refresh") || session.refreshToken;
	const csrfToken = cookieValue(response, "aster_csrf") || session.csrfToken;
	if (!accessToken) {
		fail("auth.refresh did not return an access cookie");
	}

	const expiresIn = Number(body?.data?.expires_in ?? session.expiresIn);
	return {
		accessToken,
		refreshToken,
		csrfToken,
		expiresAt: Date.now() + expiresIn * 1000,
		expiresIn,
		lastDuration: response.timings.duration,
	};
}

export function maybeRefreshSession(session, skewMs = 30_000) {
	if (!session || Date.now() + skewMs < session.expiresAt) {
		return session;
	}

	return refreshSession(session);
}

export function listFolder(session, folderId = null, query = {}) {
	const path =
		folderId === null
			? `/api/v1/folders${buildQuery(query)}`
			: `/api/v1/folders/${folderId}${buildQuery(query)}`;
	const response = http.get(url(path), {
		headers: authHeaders(session),
	});
	const body = assertApi(response, "folders.list", 200);
	return { response, body };
}

export function resolveRootFolderId(session, name) {
	let cursorValue = null;
	let cursorId = null;

	for (;;) {
		const { body } = listFolder(session, null, {
			folder_limit: 1000,
			file_limit: 0,
			sort_by: "name",
			sort_order: "asc",
			folder_after_value: cursorValue,
			folder_after_id: cursorId,
		});
		const match = body.data.folders.find((folder) => folder.name === name);
		if (match) {
			return match.id;
		}

		if (!body.data.next_folder_cursor) {
			const { body: searchBody } = search(session, {
				q: name,
				limit: 20,
			});
			const searchedMatch = searchBody.data.folders.find(
				(folder) => folder.name === name,
			);
			return searchedMatch ? searchedMatch.id : null;
		}

		cursorValue = body.data.next_folder_cursor.value;
		cursorId = body.data.next_folder_cursor.id;
	}
}

export function createFolder(session, name, parentId = null) {
	const response = http.post(
		url("/api/v1/folders"),
		JSON.stringify({
			name,
			parent_id: parentId,
		}),
		{
			headers: authHeaders(session, {
				"Content-Type": "application/json",
			}),
		},
	);
	const body = assertApi(response, "folders.create", 201);
	return { response, body };
}

export function ensureRootFolder(session, name) {
	const existing = resolveRootFolderId(session, name);
	if (existing) {
		return existing;
	}

	return createFolder(session, name, null).body.data.id;
}

export function findFileEntryInFolder(session, folderId, filename) {
	let cursorValue = null;
	let cursorId = null;

	for (;;) {
		const { body } = listFolder(session, folderId, {
			folder_limit: 0,
			file_limit: 1000,
			sort_by: "name",
			sort_order: "asc",
			file_after_value: cursorValue,
			file_after_id: cursorId,
		});

		const match = body.data.files.find((file) => file.name === filename);
		if (match) {
			return match;
		}

		if (!body.data.next_file_cursor) {
			return null;
		}

		cursorValue = body.data.next_file_cursor.value;
		cursorId = body.data.next_file_cursor.id;
	}
}

export function findFileInFolder(session, folderId, filename) {
	const file = findFileEntryInFolder(session, folderId, filename);
	return file ? file.id : null;
}

export function uploadDirect(
	session,
	{
		filename,
		content,
		mimeType = "text/plain",
		folderId = null,
		relativePath = null,
	},
) {
	const payload = {
		file: http.file(content, filename, mimeType),
	};
	const response = http.post(
		url(
			`/api/v1/files/upload${buildQuery({
				folder_id: folderId,
				relative_path: relativePath,
			})}`,
		),
		payload,
		{
			headers: authHeaders(session),
		},
	);
	const body = assertApi(response, "files.upload_direct", 201);
	return { response, body };
}

export function initChunkedUpload(
	session,
	{ filename, totalSize, folderId = null, relativePath = null },
) {
	const response = http.post(
		url("/api/v1/files/upload/init"),
		JSON.stringify({
			filename,
			total_size: totalSize,
			folder_id: folderId,
			relative_path: relativePath,
		}),
		{
			headers: authHeaders(session, {
				"Content-Type": "application/json",
			}),
		},
	);
	const body = assertApi(response, "files.upload_init", 201);
	return { response, body };
}

export function uploadChunk(session, uploadId, chunkNumber, body) {
	const response = http.put(
		url(`/api/v1/files/upload/${uploadId}/${chunkNumber}`),
		body,
		{
			headers: authHeaders(session, {
				"Content-Type": "application/octet-stream",
			}),
		},
	);
	const json = assertApi(response, "files.upload_chunk", 200);
	return { response, body: json };
}

export function completeUpload(session, uploadId) {
	const response = http.post(
		url(`/api/v1/files/upload/${uploadId}/complete`),
		null,
		{
			headers: authHeaders(session),
		},
	);
	const body = assertApi(response, "files.upload_complete", 201);
	return { response, body };
}

export function search(session, query = {}) {
	const response = http.get(url(`/api/v1/search${buildQuery(query)}`), {
		headers: authHeaders(session),
	});
	const body = assertApi(response, "search", 200);
	return { response, body };
}

export function downloadFile(session, fileId, headers = {}) {
	const response = http.get(url(`/api/v1/files/${fileId}/download`), {
		headers: authHeaders(session, headers),
		responseType: "none",
	});
	check(response, {
		"files.download: status 200": (resp) => resp.status === 200,
	}) || fail(`files.download failed: ${response.status}`);
	return response;
}

export function downloadFileRange(session, fileId, start, end) {
	const response = http.get(url(`/api/v1/files/${fileId}/download`), {
		headers: authHeaders(session, {
			Range: `bytes=${start}-${end}`,
		}),
		responseType: "none",
	});
	check(response, {
		"files.download range: status 206": (resp) => resp.status === 206,
		"files.download range: content-range": (resp) =>
			resp.headers["Content-Range"] === `bytes ${start}-${end}/*` ||
			resp.headers["Content-Range"]?.startsWith(`bytes ${start}-${end}/`),
	}) ||
		fail(
			`files.download range failed: ${response.status} ${response.headers["Content-Range"]}`,
		);
	return response;
}

export function getThumbnail(session, fileId) {
	const response = http.get(url(`/api/v1/files/${fileId}/thumbnail`), {
		headers: authHeaders(session),
		responseType: "none",
	});
	check(response, {
		"files.thumbnail: status 200 or 202": (resp) =>
			resp.status === 200 || resp.status === 202 || resp.status === 304,
	}) || fail(`files.thumbnail failed: ${response.status}`);
	return response;
}

export function batchMove(session, fileIds, folderIds, targetFolderId) {
	const response = http.post(
		url("/api/v1/batch/move"),
		JSON.stringify({
			file_ids: fileIds,
			folder_ids: folderIds,
			target_folder_id: targetFolderId,
		}),
		{
			headers: authHeaders(session, {
				"Content-Type": "application/json",
			}),
		},
	);
	const body = assertApi(response, "batch.move", 200);
	return { response, body };
}

export function createArchiveCompressTask(
	session,
	{ fileIds = [], folderIds = [], archiveName, targetFolderId = null },
) {
	const response = http.post(
		url("/api/v1/batch/archive-compress"),
		JSON.stringify({
			file_ids: fileIds,
			folder_ids: folderIds,
			archive_name: archiveName,
			target_folder_id: targetFolderId,
		}),
		{
			headers: authHeaders(session, {
				"Content-Type": "application/json",
			}),
		},
	);
	const body = assertApi(response, "batch.archive_compress", 200);
	return { response, body };
}

export function createStoragePolicyMigrationTask(
	session,
	{ sourcePolicyId, targetPolicyId, deleteSourceAfterSuccess = false },
) {
	const response = http.post(
		url("/api/v1/admin/storage-migrations"),
		JSON.stringify({
			source_policy_id: sourcePolicyId,
			target_policy_id: targetPolicyId,
			delete_source_after_success: deleteSourceAfterSuccess,
		}),
		{
			headers: authHeaders(session, {
				"Content-Type": "application/json",
			}),
		},
	);
	const body = assertApi(response, "admin.storage_migrations.create", 200);
	return { response, body };
}

export function listAdminTasks(session, query = {}) {
	const response = http.get(url(`/api/v1/admin/tasks${buildQuery(query)}`), {
		headers: authHeaders(session),
	});
	const body = assertApi(response, "admin.tasks.list", 200);
	return { response, body };
}

export function listWebdavAccounts(session) {
	const response = http.get(url("/api/v1/webdav-accounts?limit=100&offset=0"), {
		headers: authHeaders(session),
	});
	const body = assertApi(response, "webdav.accounts.list", 200);
	return { response, body };
}

export function webdavRequest(
	method,
	path,
	body = null,
	{
		username = benchConfig.webdavUsername,
		password = benchConfig.webdavPassword,
		headers = {},
		responseType,
	} = {},
) {
	const encodedPath = path
		.split("/")
		.filter(Boolean)
		.map((segment) => encodeURIComponent(segment))
		.join("/");
	const prefix = benchConfig.webdavPrefix.replace(/\/+$/, "");
	const target = encodedPath ? `${prefix}/${encodedPath}` : `${prefix}/`;
	const auth = encoding.b64encode(`${username}:${password}`);

	const params = {
		headers: {
			Authorization: `Basic ${auth}`,
			...headers,
		},
	};
	if (responseType !== undefined) {
		params.responseType = responseType;
	}

	return http.request(method, url(target), body, params);
}

export function uniqueName(prefix, extension = "txt") {
	return `${prefix}-${exec.vu.idInTest}-${exec.scenario.iterationInTest}.${extension}`;
}
