import { beforeEach, describe, expect, it, vi } from "vitest";
import {
	appendCompletedPart,
	clearAllSessions,
	loadPendingEmptyFiles,
	loadSessions,
	type ResumableSession,
	removePendingEmptyFile,
	removeSession,
	savePendingEmptyFiles,
	saveSession,
} from "@/lib/uploadPersistence";
import type { Workspace } from "@/lib/workspace";

function createSession(
	overrides: Partial<ResumableSession> = {},
): ResumableSession {
	return {
		uploadId: "upload-1",
		filename: "hello.txt",
		totalSize: 12,
		totalChunks: 3,
		chunkSize: 4,
		baseFolderId: 42,
		baseFolderName: "Projects",
		relativePath: null,
		savedAt: Date.now(),
		mode: "chunked",
		...overrides,
	};
}

const TEAM_WORKSPACE: Workspace = { kind: "team", teamId: 9 };

describe("uploadPersistence", () => {
	beforeEach(() => {
		localStorage.clear();
		vi.restoreAllMocks();
	});

	it("saves, replaces, and removes sessions by upload id", () => {
		saveSession(createSession());
		saveSession(
			createSession({
				uploadId: "upload-2",
				filename: "world.txt",
			}),
		);
		saveSession(
			createSession({
				uploadId: "upload-1",
				filename: "hello-v2.txt",
			}),
		);

		expect(loadSessions()).toEqual([
			expect.objectContaining({
				uploadId: "upload-2",
				filename: "world.txt",
			}),
			expect.objectContaining({
				uploadId: "upload-1",
				filename: "hello-v2.txt",
			}),
		]);

		removeSession("upload-2");

		expect(loadSessions()).toEqual([
			expect.objectContaining({
				uploadId: "upload-1",
				filename: "hello-v2.txt",
			}),
		]);
	});

	it("tracks completed parts without duplicating the same part number", () => {
		saveSession(
			createSession({
				mode: "presigned_multipart",
				completedParts: [{ part_number: 1, etag: "etag-1" }],
			}),
		);

		appendCompletedPart("upload-1", {
			part_number: 2,
			etag: "etag-2",
		});
		appendCompletedPart("upload-1", {
			part_number: 2,
			etag: "etag-2-duplicate",
		});

		expect(loadSessions()).toEqual([
			expect.objectContaining({
				completedParts: [
					{ part_number: 1, etag: "etag-1" },
					{ part_number: 2, etag: "etag-2" },
				],
			}),
		]);
	});

	it("drops expired sessions when loading", () => {
		const now = 10_000_000;
		vi.spyOn(Date, "now").mockReturnValue(now);

		saveSession(
			createSession({
				uploadId: "fresh",
				savedAt: now - (23 * 60 * 60 * 1000 - 1),
			}),
		);
		saveSession(
			createSession({
				uploadId: "expired",
				savedAt: now - (23 * 60 * 60 * 1000 + 1),
			}),
		);

		expect(loadSessions()).toEqual([
			expect.objectContaining({
				uploadId: "fresh",
			}),
		]);
		expect(loadSessions()).toEqual([
			expect.objectContaining({
				uploadId: "fresh",
			}),
		]);
	});

	it("clears all persisted sessions", () => {
		saveSession(createSession());

		clearAllSessions();

		expect(loadSessions()).toEqual([]);
	});

	it("filters sessions by workspace when requested", () => {
		saveSession(createSession({ uploadId: "personal-1" }));
		saveSession(
			createSession({
				uploadId: "team-1",
				workspace: TEAM_WORKSPACE,
			}),
		);

		expect(loadSessions()).toHaveLength(2);
		expect(loadSessions(TEAM_WORKSPACE)).toEqual([
			expect.objectContaining({
				uploadId: "team-1",
			}),
		]);
	});

	it("persists empty-file replay keys by workspace and removes them explicitly", () => {
		savePendingEmptyFiles([
			{
				taskId: "empty-personal",
				idempotencyKey: "key-personal",
				filename: "empty.txt",
				baseFolderId: null,
				baseFolderName: "Root",
				relativePath: null,
				savedAt: Date.now(),
			},
		]);
		savePendingEmptyFiles([
			{
				taskId: "empty-team",
				idempotencyKey: "key-team",
				filename: "nested.txt",
				baseFolderId: 5,
				baseFolderName: "Docs",
				relativePath: "nested/empty.txt",
				savedAt: Date.now(),
				workspace: TEAM_WORKSPACE,
			},
		]);

		expect(loadPendingEmptyFiles(TEAM_WORKSPACE)).toEqual([
			expect.objectContaining({
				taskId: "empty-team",
				idempotencyKey: "key-team",
			}),
		]);
		removePendingEmptyFile("empty-team");
		expect(loadPendingEmptyFiles(TEAM_WORKSPACE)).toEqual([]);
		expect(loadPendingEmptyFiles()).toHaveLength(1);
	});

	it("persists a large empty-file batch with one storage write", () => {
		localStorage.setItem(
			"aster_pending_empty_file_creates",
			JSON.stringify([
				{
					taskId: "preserved",
					idempotencyKey: "preserved-key",
					filename: "preserved.txt",
					baseFolderId: null,
					baseFolderName: "Root",
					relativePath: null,
					savedAt: Date.now(),
					workspace: { kind: "personal" },
				},
				{
					taskId: "empty-0",
					idempotencyKey: "stale-key",
					filename: "stale.txt",
					baseFolderId: null,
					baseFolderName: "Root",
					relativePath: null,
					savedAt: Date.now(),
					workspace: { kind: "personal" },
				},
			]),
		);
		const setItemSpy = vi.spyOn(Storage.prototype, "setItem");
		const savedAt = Date.now();
		const batch = Array.from({ length: 1_000 }, (_, index) => ({
			taskId: `empty-${index}`,
			idempotencyKey: `key-${index}`,
			filename: `${index}.txt`,
			baseFolderId: null,
			baseFolderName: "Root",
			relativePath: null,
			savedAt,
		}));

		savePendingEmptyFiles(batch);

		expect(setItemSpy).toHaveBeenCalledTimes(1);
		expect(loadPendingEmptyFiles()).toHaveLength(1_001);
		expect(
			loadPendingEmptyFiles().find((entry) => entry.taskId === "empty-0"),
		).toEqual(expect.objectContaining({ idempotencyKey: "key-0" }));
	});

	it("drops malformed pending empty-file records before workspace filtering", () => {
		localStorage.setItem(
			"aster_pending_empty_file_creates",
			JSON.stringify([
				{
					taskId: "valid",
					idempotencyKey: "valid-key",
					filename: "valid.txt",
					baseFolderId: null,
					baseFolderName: "Root",
					relativePath: null,
					savedAt: Date.now(),
				},
				{
					taskId: "missing-key",
					filename: "missing-key.txt",
					baseFolderId: null,
					baseFolderName: "Root",
					relativePath: null,
					savedAt: Date.now(),
				},
				{
					taskId: "invalid-workspace",
					idempotencyKey: "invalid-workspace-key",
					filename: "invalid-workspace.txt",
					baseFolderId: null,
					baseFolderName: "Root",
					relativePath: null,
					savedAt: Date.now(),
					workspace: "personal",
				},
				null,
			]),
		);

		expect(loadPendingEmptyFiles({ kind: "personal" })).toEqual([
			expect.objectContaining({
				taskId: "valid",
				idempotencyKey: "valid-key",
			}),
		]);
		expect(
			JSON.parse(
				localStorage.getItem("aster_pending_empty_file_creates") ?? "[]",
			),
		).toEqual([expect.objectContaining({ taskId: "valid" })]);
	});

	it("clears malformed pending empty-file payloads without interrupting uploads", () => {
		localStorage.setItem("aster_pending_empty_file_creates", "{}");
		expect(loadPendingEmptyFiles()).toEqual([]);
		expect(localStorage.getItem("aster_pending_empty_file_creates")).toBeNull();

		localStorage.setItem("aster_pending_empty_file_creates", "{");
		expect(loadPendingEmptyFiles()).toEqual([]);
		expect(localStorage.getItem("aster_pending_empty_file_creates")).toBeNull();

		vi.spyOn(Storage.prototype, "setItem").mockImplementation(() => {
			throw new DOMException("storage unavailable", "InvalidStateError");
		});
		expect(() =>
			savePendingEmptyFiles([
				{
					taskId: "write-failure",
					idempotencyKey: "write-failure-key",
					filename: "write-failure.txt",
					baseFolderId: null,
					baseFolderName: "Root",
					relativePath: null,
					savedAt: Date.now(),
				},
			]),
		).not.toThrow();
	});

	it("drops empty-file replay records before the server retention expires", () => {
		const now = 50_000_000;
		vi.spyOn(Date, "now").mockReturnValue(now);
		for (const [taskId, savedAt] of [
			["fresh-empty", now - (23 * 60 * 60 * 1000 - 1)],
			["expired-empty", now - (23 * 60 * 60 * 1000 + 1)],
		] as const) {
			savePendingEmptyFiles([
				{
					taskId,
					idempotencyKey: `${taskId}-key`,
					filename: `${taskId}.txt`,
					baseFolderId: null,
					baseFolderName: "Root",
					relativePath: null,
					savedAt,
				},
			]);
		}

		expect(loadPendingEmptyFiles()).toEqual([
			expect.objectContaining({ taskId: "fresh-empty" }),
		]);
	});

	it("prunes expired empty-file replay records while saving a new batch", () => {
		const now = 50_000_000;
		vi.spyOn(Date, "now").mockReturnValue(now);
		localStorage.setItem(
			"aster_pending_empty_file_creates",
			JSON.stringify([
				{
					taskId: "expired-empty",
					idempotencyKey: "expired-key",
					filename: "expired.txt",
					baseFolderId: null,
					baseFolderName: "Root",
					relativePath: null,
					savedAt: now - (23 * 60 * 60 * 1000 + 1),
				},
			]),
		);

		savePendingEmptyFiles([
			{
				taskId: "fresh-empty",
				idempotencyKey: "fresh-key",
				filename: "fresh.txt",
				baseFolderId: null,
				baseFolderName: "Root",
				relativePath: null,
				savedAt: now,
			},
		]);

		expect(loadPendingEmptyFiles()).toEqual([
			expect.objectContaining({ taskId: "fresh-empty" }),
		]);
	});

	it("trims older sessions when localStorage quota is exceeded", () => {
		// 先正常存 4 个 session（按 savedAt 0/1/2/3 升序）
		for (let i = 0; i < 4; i += 1) {
			saveSession(
				createSession({
					uploadId: `upload-${i}`,
					savedAt: i,
				}),
			);
		}

		// mock setItem：第 1 次抛 QuotaExceededError，第 2 次成功
		let attempt = 0;
		const setItemSpy = vi
			.spyOn(Storage.prototype, "setItem")
			.mockImplementation(function (this: Storage, key: string, value: string) {
				attempt += 1;
				if (attempt === 1) {
					throw new DOMException("quota", "QuotaExceededError");
				}
				// 第 2 次走原始实现（vi 的 spy 默认替换，需要手动落盘）
				const storage = this as unknown as {
					__store?: Record<string, string>;
				};
				if (storage.__store == null) {
					storage.__store = {};
				}
				const obj = storage.__store;
				obj[key] = value;
			});

		// 触发第 5 次 save → 命中 quota → trim 后重试
		saveSession(
			createSession({
				uploadId: "upload-new",
				savedAt: 100,
			}),
		);

		expect(setItemSpy).toHaveBeenCalledTimes(2);
		// 第 2 次写入的 payload 应该已经裁掉一半旧 session
		const secondCallPayload = setItemSpy.mock.calls[1]?.[1] as string;
		const persisted = JSON.parse(secondCallPayload) as ResumableSession[];
		// 5 条 → floor(5/2) = 2 条；按 savedAt desc 保留最新的 upload-new (100) + upload-3 (3)
		expect(persisted).toHaveLength(2);
		expect(persisted.map((s) => s.uploadId)).toEqual([
			"upload-new",
			"upload-3",
		]);
	});

	it("clears storage when quota persists even after trimming", () => {
		saveSession(createSession({ uploadId: "u1", savedAt: 1 }));

		const setItemSpy = vi
			.spyOn(Storage.prototype, "setItem")
			.mockImplementation(() => {
				throw new DOMException("quota", "QuotaExceededError");
			});
		const removeItemSpy = vi.spyOn(Storage.prototype, "removeItem");

		// 不应抛出，整页不能因为一次写入失败而 crash
		expect(() => {
			saveSession(createSession({ uploadId: "u2", savedAt: 2 }));
		}).not.toThrow();

		// 第 1 次直接写、第 2 次 trim 后重写都失败 → 走 removeItem 兜底
		expect(setItemSpy).toHaveBeenCalledTimes(2);
		expect(removeItemSpy).toHaveBeenCalledWith("aster_resumable_uploads");
	});

	it("ignores non-quota DOMExceptions without crashing", () => {
		vi.spyOn(Storage.prototype, "setItem").mockImplementation(() => {
			throw new DOMException("nope", "InvalidStateError");
		});

		expect(() => {
			saveSession(createSession({ uploadId: "u-ignored" }));
		}).not.toThrow();
	});
});
