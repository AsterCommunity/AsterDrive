import { describe, expect, it } from "vitest";
import { ApiError } from "@/services/http";
import type { ArchivePreviewManifest } from "@/types/api";
import { ApiSubcode, ErrorCode } from "@/types/api-helpers";
import {
	buildArchiveBreadcrumb,
	buildArchiveDirectoryEntries,
	buildArchiveVisibleEntries,
	classifyArchivePreviewError,
	parentPathForArchivePath,
} from "./archivePreviewUtils";

const baseManifest: ArchivePreviewManifest = {
	schema_version: 2,
	format: "zip",
	source_blob_id: 1,
	source_hash: "hash",
	generated_at: "2026-01-02T03:04:05Z",
	entry_count: 3,
	file_count: 2,
	directory_count: 1,
	total_uncompressed_size: 12,
	truncated: false,
	extract_compatibility: {
		supported: true,
		reason: null,
	},
	entries: [
		{
			path: "docs/readme.txt",
			name: "readme.txt",
			parent: "docs",
			kind: "file",
			size: 5,
			compressed_size: 5,
			modified_at: null,
		},
		{
			path: "docs/images/logo.png",
			name: "logo.png",
			parent: "docs/images",
			kind: "file",
			size: 7,
			compressed_size: 7,
			modified_at: null,
		},
		{
			path: "root.txt",
			name: "root.txt",
			parent: null,
			kind: "file",
			size: 1,
			compressed_size: 1,
			modified_at: null,
		},
	],
};

describe("archivePreviewUtils", () => {
	it("creates synthetic directory entries for implicit parents", () => {
		const directories = buildArchiveDirectoryEntries(baseManifest.entries);

		expect(Array.from(directories.keys())).toEqual(["docs", "docs/images"]);
		expect(directories.get("docs")).toMatchObject({
			kind: "directory",
			name: "docs",
			parent: null,
			synthetic: true,
		});
		expect(directories.get("docs/images")).toMatchObject({
			kind: "directory",
			name: "images",
			parent: "docs",
			synthetic: true,
		});
	});

	it("builds root, folder, and search views without duplicate explicit folders", () => {
		const manifest: ArchivePreviewManifest = {
			...baseManifest,
			entries: [
				{
					path: "docs",
					name: "docs",
					parent: null,
					kind: "directory",
					size: 0,
					compressed_size: 0,
					modified_at: null,
				},
				...baseManifest.entries,
			],
		};
		const directories = buildArchiveDirectoryEntries(manifest.entries);

		expect(
			buildArchiveVisibleEntries(manifest, directories, "", null).map(
				(entry) => entry.path,
			),
		).toEqual(["docs", "root.txt"]);
		expect(
			buildArchiveVisibleEntries(manifest, directories, "", "docs").map(
				(entry) => entry.path,
			),
		).toEqual(["docs/images", "docs/readme.txt"]);
		expect(
			buildArchiveVisibleEntries(manifest, directories, "logo", "docs").map(
				(entry) => entry.path,
			),
		).toEqual(["docs/images/logo.png"]);
	});

	it("builds breadcrumbs and parent paths from normalized archive paths", () => {
		expect(parentPathForArchivePath("docs/images/")).toBe("docs");
		expect(parentPathForArchivePath("docs")).toBeNull();
		expect(buildArchiveBreadcrumb("docs/images", "root")).toEqual([
			{ path: null, name: "root" },
			{ path: "docs", name: "docs" },
			{ path: "docs/images", name: "images" },
		]);
	});

	it("classifies invalid filename encoding errors separately from generic rejected archives", () => {
		const error = new ApiError(
			ErrorCode.BadRequest,
			"archive entry 'x.drawio' filename is not valid Big5",
			{
				internalCode: "E005",
				subcode: ApiSubcode.ArchivePreviewRejected,
			},
		);

		expect(classifyArchivePreviewError(error)).toBe("encoding");
		expect(
			classifyArchivePreviewError(
				new ApiError(
					ErrorCode.BadRequest,
					"archive contains 2 entries, exceeds server limit 1",
					{
						internalCode: "E005",
						subcode: ApiSubcode.ArchivePreviewRejected,
					},
				),
			),
		).toBe("rejected");
	});
});
