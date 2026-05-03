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
	mode?: "chunked" | "presigned" | "presigned_multipart";
	/** S3 multipart: 已上传 part 的 {partNumber, etag} */
	completedParts?: { part_number: number; etag: string }[];
}

const STORAGE_KEY = "aster_resumable_uploads";
/** 23h — 留 1h 余量，服务器 session 24h 过期 */
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
		const sorted = [...sessions].sort((a, b) => b.savedAt - a.savedAt);
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

/** 清空所有 session */
export function clearAllSessions(): void {
	localStorage.removeItem(STORAGE_KEY);
}
