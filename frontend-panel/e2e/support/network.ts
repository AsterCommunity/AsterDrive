import { expect, type Page } from "@playwright/test";
import { type E2eApiResponse, expectApiSuccess } from "./api-response";
import { RESUMABLE_UPLOADS_KEY } from "./fixtures";

export async function apiJsonInPage<T>(
	page: Page,
	requestPath: string,
	options?: {
		body?: unknown;
		method?: string;
		withCsrf?: boolean;
	},
) {
	const response = await page.evaluate(
		async ({ body, method, requestPath, withCsrf }) => {
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

			const headers: Record<string, string> = {};
			if (body !== undefined) {
				headers["Content-Type"] = "application/json";
			}
			if (withCsrf) {
				const token = readCookie("aster_csrf");
				if (token) {
					headers["X-CSRF-Token"] = token;
				}
			}

			const result = await fetch(requestPath, {
				body: body === undefined ? undefined : JSON.stringify(body),
				credentials: "include",
				headers,
				method,
			});

			return {
				status: result.status,
				text: await result.text(),
			};
		},
		{
			body: options?.body,
			method: options?.method ?? "GET",
			requestPath,
			withCsrf: options?.withCsrf ?? false,
		},
	);

	expect(response.status).toBeGreaterThanOrEqual(200);
	expect(response.status).toBeLessThan(300);
	const payload = JSON.parse(response.text) as E2eApiResponse<T>;
	expectApiSuccess(payload);
	return payload.data;
}

export async function uploadChunkViaApi(
	page: Page,
	uploadId: string,
	chunkNumber: number,
	buffer: Buffer,
) {
	const response = await page.evaluate(
		async ({ bufferBase64, chunkNumber, uploadId }) => {
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
			const headers: Record<string, string> = {
				"Content-Type": "application/octet-stream",
			};
			const csrfToken = readCookie("aster_csrf");
			if (csrfToken) {
				headers["X-CSRF-Token"] = csrfToken;
			}

			const result = await fetch(
				`/api/v1/files/upload/${uploadId}/${chunkNumber}`,
				{
					body: bytes,
					credentials: "include",
					headers,
					method: "PUT",
				},
			);

			return {
				status: result.status,
				text: await result.text(),
			};
		},
		{
			bufferBase64: buffer.toString("base64"),
			chunkNumber,
			uploadId,
		},
	);

	expect(response.status).toBeGreaterThanOrEqual(200);
	expect(response.status).toBeLessThan(300);
	const payload = JSON.parse(response.text) as E2eApiResponse;
	expectApiSuccess(payload);
}

export async function saveResumableSession(
	page: Page,
	session: {
		baseFolderId: number | null;
		baseFolderName: string;
		chunkSize: number;
		filename: string;
		mode: "chunked" | "presigned_multipart" | "provider_resumable";
		relativePath: string | null;
		savedAt: number;
		totalChunks: number;
		totalSize: number;
		uploadId: string;
		workspace: { kind: "personal" } | { kind: "team"; teamId: number };
	},
) {
	await page.evaluate(
		({ session, storageKey }) => {
			const raw = window.localStorage.getItem(storageKey);
			const existing = raw ? (JSON.parse(raw) as unknown[]) : [];
			const next = existing.filter((item) => {
				if (
					typeof item === "object" &&
					item !== null &&
					"uploadId" in item &&
					typeof item.uploadId === "string"
				) {
					return item.uploadId !== session.uploadId;
				}
				return true;
			});
			next.push(session);
			window.localStorage.setItem(storageKey, JSON.stringify(next));
		},
		{
			session,
			storageKey: RESUMABLE_UPLOADS_KEY,
		},
	);
}

export async function loadPersistedSessions(page: Page) {
	return page.evaluate((storageKey) => {
		const raw = window.localStorage.getItem(storageKey);
		return raw ? (JSON.parse(raw) as unknown[]) : [];
	}, RESUMABLE_UPLOADS_KEY);
}

export function basicAuth(username: string, password: string) {
	return `Basic ${Buffer.from(`${username}:${password}`, "utf8").toString("base64")}`;
}

export function normalizeWebdavPrefix(prefix: string) {
	const trimmed = prefix.trim();
	if (!trimmed) {
		return "/webdav";
	}

	if (trimmed === "/") {
		return "";
	}

	return trimmed.startsWith("/")
		? trimmed.replace(/\/+$/, "")
		: `/${trimmed.replace(/\/+$/, "")}`;
}

export async function webdavRequest(
	page: Page,
	requestPath: string,
	options: {
		body?: string;
		headers?: Record<string, string>;
		method:
			| "COPY"
			| "DELETE"
			| "GET"
			| "LOCK"
			| "MKCOL"
			| "MOVE"
			| "PROPFIND"
			| "PUT";
	},
) {
	return page.evaluate(
		async ({ body, headers, method, requestPath }) => {
			const response = await fetch(requestPath, {
				body,
				headers,
				method,
			});
			return {
				status: response.status,
				text: await response.text(),
			};
		},
		{
			body: options.body,
			headers: options.headers,
			method: options.method,
			requestPath,
		},
	);
}

export async function waitForApiCondition<T>(
	page: Page,
	requestPath: string,
	predicate: (data: T) => boolean,
	options?: {
		intervalMs?: number;
		timeoutMs?: number;
	},
) {
	const timeoutMs = options?.timeoutMs ?? 30_000;
	const intervalMs = options?.intervalMs ?? 750;
	const startedAt = Date.now();
	let latest: T | null = null;

	for (;;) {
		latest = await apiJsonInPage<T>(page, requestPath);
		if (predicate(latest)) {
			return latest;
		}
		if (Date.now() - startedAt >= timeoutMs) {
			throw new Error(`Timed out waiting for API condition at ${requestPath}`);
		}
		await page.waitForTimeout(intervalMs);
	}
}
