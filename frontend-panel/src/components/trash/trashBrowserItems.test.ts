import { describe, expect, it } from "vitest";
import {
	buildTrashMetaMap,
	toBrowserFiles,
	toBrowserFolders,
} from "@/components/trash/trashBrowserItems";
import type { TrashFileItem, TrashFolderItem } from "@/types/api";

const unlocked = { state: "unlocked" } as TrashFileItem["lock_state"];

function makeFile(overrides: Partial<TrashFileItem> = {}): TrashFileItem {
	return {
		created_at: "2026-04-01T00:00:00Z",
		expires_at: "2026-04-08T00:00:00Z",
		id: 1,
		lock_state: unlocked,
		mime_type: "application/pdf",
		name: "report.pdf",
		original_path: "/Docs",
		size: 128,
		updated_at: "2026-04-02T00:00:00Z",
		...overrides,
	};
}

function makeFolder(overrides: Partial<TrashFolderItem> = {}): TrashFolderItem {
	return {
		created_at: "2026-04-01T00:00:00Z",
		expires_at: "2026-04-08T00:00:00Z",
		id: 7,
		lock_state: unlocked,
		name: "Projects",
		original_path: "/",
		updated_at: "2026-04-02T00:00:00Z",
		...overrides,
	};
}

describe("trashBrowserItems", () => {
	it("maps trash files into file list items with derived extension and category", () => {
		const files = toBrowserFiles([
			makeFile({ id: 1, name: "report.pdf", mime_type: "application/pdf" }),
			makeFile({ id: 2, name: "photo.JPG", mime_type: "image/jpeg" }),
			makeFile({ id: 3, name: "README", mime_type: "text/plain" }),
		]);

		expect(files.map((file) => file.id)).toEqual([1, 2, 3]);
		expect(files[0]).toMatchObject({
			extension: "pdf",
			file_category: "document",
			is_shared: false,
			mime_type: "application/pdf",
			name: "report.pdf",
			size: 128,
			tags: [],
		});
		expect(files[1]).toMatchObject({
			extension: "jpg",
			file_category: "image",
		});
		// 无扩展名：extension 为空，category 仍能按 MIME 推导
		expect(files[2]?.extension).toBe("");
		expect(files[2]?.file_category).not.toBe("other");
	});

	it("sorts files and folders by expiration time descending", () => {
		const files = toBrowserFiles([
			makeFile({ id: 1, expires_at: "2026-04-01T00:00:00Z" }),
			makeFile({ id: 2, expires_at: "2026-04-09T00:00:00Z" }),
			makeFile({ id: 3, expires_at: "2026-04-05T00:00:00Z" }),
		]);
		const folders = toBrowserFolders([
			makeFolder({ id: 1, expires_at: "2026-04-02T00:00:00Z" }),
			makeFolder({ id: 2, expires_at: "2026-04-10T00:00:00Z" }),
		]);

		expect(files.map((file) => file.id)).toEqual([2, 3, 1]);
		expect(folders.map((folder) => folder.id)).toEqual([2, 1]);
	});

	it("preserves lock state and timestamps from the trash entry", () => {
		const locked = {
			state: "locked",
			lock: { id: 1 },
		} as TrashFileItem["lock_state"];
		const [file] = toBrowserFiles([
			makeFile({ lock_state: locked, updated_at: "2026-04-03T10:00:00Z" }),
		]);
		expect(file?.lock_state).toBe(locked);
		expect(file?.updated_at).toBe("2026-04-03T10:00:00Z");
	});

	it("builds a meta map keyed by entity type and id", () => {
		const map = buildTrashMetaMap({
			files: [
				makeFile({
					id: 3,
					original_path: "/Docs",
					expires_at: "2026-04-08T00:00:00Z",
				}),
			],
			folders: [
				makeFolder({
					id: 3,
					original_path: "/",
					expires_at: "2026-04-09T00:00:00Z",
				}),
			],
		});

		// 文件与文件夹 id 可以相同，key 必须带类型前缀
		expect(map.get("file:3")).toEqual({
			expiresAt: "2026-04-08T00:00:00Z",
			originalPath: "/Docs",
		});
		expect(map.get("folder:3")).toEqual({
			expiresAt: "2026-04-09T00:00:00Z",
			originalPath: "/",
		});
	});
});
