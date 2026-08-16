import { logger } from "@/lib/logger";
import {
	PERSONAL_WORKSPACE,
	type Workspace,
	workspaceEquals,
} from "@/lib/workspace";

/**
 * 断点续传持久化层
 * 将 chunked upload session 元数据存入 localStorage，刷新后可恢复。
 */

export interface ResumableSession {
	uploadId: string;
	filename: string;
	totalSize: number;
	totalChunks: number;
	chunkSize: number;
	baseFolderId: number | null;
	baseFolderName: string;
	relativePath: string | null;
	savedAt: number;
	workspace?: Workspace;
	/** 可恢复上传模式。direct 没有 session，不会写入这里。 */
	mode?: "chunked" | "presigned" | "presigned_multipart" | "provider_resumable";
	/** S3 multipart: 已上传 part 的 {partNumber, etag} */
	completedParts?: { part_number: number; etag: string }[];
}

export interface PendingEmptyFileCreate {
	taskId: string;
	idempotencyKey: string;
	filename: string;
	baseFolderId: number | null;
	baseFolderName: string;
	relativePath: string | null;
	savedAt: number;
	workspace?: Workspace;
}

const STORAGE_KEY = "aster_resumable_uploads";
const EMPTY_FILE_STORAGE_KEY = "aster_pending_empty_file_creates";
/** 23h: leave one hour before the server's 24h idempotency retention expires. */
const MAX_AGE_MS = 23 * 60 * 60 * 1000;

function readAll(): ResumableSession[] {
	try {
		const raw = localStorage.getItem(STORAGE_KEY);
		if (!raw) return [];
		return JSON.parse(raw) as ResumableSession[];
	} catch {
		return [];
	}
}

interface PendingEmptyFileReadResult {
	entries: PendingEmptyFileCreate[];
	needsCleanup: boolean;
}

function isWorkspace(value: unknown): value is Workspace {
	if (typeof value !== "object" || value === null || !("kind" in value)) {
		return false;
	}
	if (value.kind === "personal") return true;
	return (
		value.kind === "team" &&
		"teamId" in value &&
		typeof value.teamId === "number" &&
		Number.isFinite(value.teamId)
	);
}

function isPendingEmptyFileCreate(
	value: unknown,
): value is PendingEmptyFileCreate {
	if (typeof value !== "object" || value === null) return false;
	const entry = value as Record<string, unknown>;
	return (
		typeof entry.taskId === "string" &&
		entry.taskId.length > 0 &&
		typeof entry.idempotencyKey === "string" &&
		entry.idempotencyKey.length > 0 &&
		typeof entry.filename === "string" &&
		(entry.baseFolderId === null ||
			(typeof entry.baseFolderId === "number" &&
				Number.isFinite(entry.baseFolderId))) &&
		typeof entry.baseFolderName === "string" &&
		(entry.relativePath === null || typeof entry.relativePath === "string") &&
		typeof entry.savedAt === "number" &&
		Number.isFinite(entry.savedAt) &&
		(entry.workspace === undefined || isWorkspace(entry.workspace))
	);
}

function readPendingEmptyFiles(): PendingEmptyFileReadResult {
	try {
		const raw = localStorage.getItem(EMPTY_FILE_STORAGE_KEY);
		if (!raw) return { entries: [], needsCleanup: false };
		const parsed: unknown = JSON.parse(raw);
		if (!Array.isArray(parsed)) {
			return { entries: [], needsCleanup: true };
		}
		const entries = parsed.filter(isPendingEmptyFileCreate);
		return {
			entries,
			needsCleanup: entries.length !== parsed.length,
		};
	} catch {
		return { entries: [], needsCleanup: true };
	}
}

function writePendingEmptyFiles(entries: PendingEmptyFileCreate[]): void {
	try {
		if (entries.length === 0) {
			localStorage.removeItem(EMPTY_FILE_STORAGE_KEY);
			return;
		}
		localStorage.setItem(EMPTY_FILE_STORAGE_KEY, JSON.stringify(entries));
	} catch (error) {
		logger.warn("failed to persist pending empty-file creates", error);
	}
}

/** 检测错误是否为 localStorage 配额超限。各浏览器 DOMException name 不一致。 */
function isQuotaExceededError(error: unknown): boolean {
	if (!(error instanceof DOMException)) return false;
	// `DOMException.code` 已 deprecated，只用 name 比较；
	// QuotaExceededError = Chrome/Safari，NS_ERROR_DOM_QUOTA_REACHED = Firefox 旧版。
	return (
		error.name === "QuotaExceededError" ||
		error.name === "NS_ERROR_DOM_QUOTA_REACHED"
	);
}

/**
 * 写入 localStorage，遇到 QuotaExceededError 时优雅降级：
 * 1. 第一次 quota 错：丢弃最旧的一半 session 后重试（保留更近的恢复机会）
 * 2. 仍失败：清空整个 key 并 warn，避免一次写失败让整页 crash
 *
 * 真实场景触发条件：多 workspace + 大量并发分片上传 + 巨型 completedParts 数组
 * 累积体积可能突破浏览器 5–10MB 的 origin quota。
 */
function writeAll(sessions: ResumableSession[]): void {
	if (sessions.length === 0) {
		try {
			localStorage.removeItem(STORAGE_KEY);
		} catch (error) {
			logger.warn("failed to remove resumable uploads", error);
		}
		return;
	}

	try {
		localStorage.setItem(STORAGE_KEY, JSON.stringify(sessions));
		return;
	} catch (error) {
		if (!isQuotaExceededError(error)) {
			logger.warn("failed to persist resumable uploads", error);
			return;
		}

		// quota 超限：按 savedAt 降序保留较新的一半，丢掉较旧的
		const sorted = sessions.toSorted((a, b) => b.savedAt - a.savedAt);
		const trimmed = sorted.slice(0, Math.max(1, Math.floor(sorted.length / 2)));
		try {
			localStorage.setItem(STORAGE_KEY, JSON.stringify(trimmed));
			logger.warn(
				`localStorage quota exceeded; dropped ${sessions.length - trimmed.length} older resumable upload sessions`,
			);
		} catch (innerError) {
			// 仍然失败：彻底放弃持久化，清空 key 防止下次读到坏 JSON
			try {
				localStorage.removeItem(STORAGE_KEY);
			} catch {
				/* ignore */
			}
			logger.warn(
				"localStorage quota still exceeded after trimming; cleared resumable uploads to prevent crash",
				innerError,
			);
		}
	}
}

function normalizeWorkspace(workspace?: Workspace): Workspace {
	return workspace ?? PERSONAL_WORKSPACE;
}

/** 保存一个 chunked upload session（init 成功后调用） */
export function saveSession(session: ResumableSession): void {
	const all = readAll().filter((s) => s.uploadId !== session.uploadId);
	all.push({
		...session,
		workspace: normalizeWorkspace(session.workspace),
	});
	writeAll(all);
}

/** 移除一个 session（complete/cancel/永久删除时调用） */
export function removeSession(uploadId: string): void {
	writeAll(readAll().filter((s) => s.uploadId !== uploadId));
}

/** 追加已完成的 part 到 session（S3 multipart 每上传完一个 part 调用） */
export function appendCompletedPart(
	uploadId: string,
	part: { part_number: number; etag: string },
): void {
	const all = readAll();
	const session = all.find((s) => s.uploadId === uploadId);
	if (!session) return;
	const parts = session.completedParts ?? [];
	if (!parts.some((p) => p.part_number === part.part_number)) {
		parts.push(part);
		session.completedParts = parts;
		writeAll(all);
	}
}

/** 加载所有未过期的 session，自动清理过期的 */
export function loadSessions(workspace?: Workspace): ResumableSession[] {
	const now = Date.now();
	const all = readAll();
	const valid = all.filter((s) => now - s.savedAt < MAX_AGE_MS);
	if (valid.length !== all.length) {
		writeAll(valid);
	}
	if (!workspace) return valid;
	return valid.filter((session) =>
		workspaceEquals(normalizeWorkspace(session.workspace), workspace),
	);
}

export function savePendingEmptyFiles(
	entries: readonly PendingEmptyFileCreate[],
): void {
	if (entries.length === 0) return;

	const nextByTaskId = new Map(
		entries.map((entry) => [
			entry.taskId,
			{
				...entry,
				workspace: normalizeWorkspace(entry.workspace),
			},
		]),
	);
	const all = readPendingEmptyFiles().entries.filter(
		(existing) => !nextByTaskId.has(existing.taskId),
	);
	all.push(...nextByTaskId.values());
	writePendingEmptyFiles(all);
}

export function removePendingEmptyFile(taskId: string): void {
	writePendingEmptyFiles(
		readPendingEmptyFiles().entries.filter((entry) => entry.taskId !== taskId),
	);
}

export function loadPendingEmptyFiles(
	workspace?: Workspace,
): PendingEmptyFileCreate[] {
	const now = Date.now();
	const { entries: all, needsCleanup } = readPendingEmptyFiles();
	const valid = all.filter((entry) => now - entry.savedAt < MAX_AGE_MS);
	if (needsCleanup || valid.length !== all.length) {
		writePendingEmptyFiles(valid);
	}
	if (!workspace) return valid;
	return valid.filter((entry) =>
		workspaceEquals(normalizeWorkspace(entry.workspace), workspace),
	);
}

/** 清空所有 session */
export function clearAllSessions(): void {
	localStorage.removeItem(STORAGE_KEY);
	localStorage.removeItem(EMPTY_FILE_STORAGE_KEY);
}
