import { describe, expect, it } from "vitest";
import type { PublicThumbnailSupport } from "@/types/api";
import { getImagePreviewNavigation } from "./imagePreviewNavigation";

const thumbnailSupport = (extensions: string[]): PublicThumbnailSupport => ({
	audio_thumbnail: { enabled: false, extensions: [] },
	extensions,
	image_preview: { enabled: extensions.length > 0, extensions },
	image_thumbnail: { enabled: extensions.length > 0, extensions },
	version: 1,
	video_thumbnail: { enabled: false, extensions: [] },
});

const files = [
	{
		id: 1,
		file_category: "image",
		mime_type: "image/png",
		name: "first.png",
	},
	{
		id: 2,
		file_category: "document",
		mime_type: "application/pdf",
		name: "notes.pdf",
	},
	{
		id: 3,
		file_category: "image",
		mime_type: "image/jpeg",
		name: "second.jpg",
	},
];

describe("imagePreviewNavigation", () => {
	it("builds previous and next image navigation from image files only", () => {
		expect(getImagePreviewNavigation(files, files[0])).toEqual({
			previousFile: files[2],
			nextFile: files[2],
		});
		expect(getImagePreviewNavigation(files, files[2])).toEqual({
			previousFile: files[0],
			nextFile: files[0],
		});
	});

	it("keeps the queue in the original file order while skipping non-images", () => {
		const mixedFiles = [
			{
				id: 1,
				file_category: "document",
				mime_type: "application/pdf",
				name: "first.pdf",
			},
			{
				id: 2,
				file_category: "image",
				mime_type: "image/png",
				name: "first-image.png",
			},
			{
				id: 3,
				file_category: "video",
				mime_type: "video/mp4",
				name: "clip.mp4",
			},
			{
				id: 4,
				file_category: "image",
				mime_type: "image/jpeg",
				name: "second-image.jpg",
			},
			{
				id: 5,
				file_category: "image",
				mime_type: "image/webp",
				name: "third-image.webp",
			},
		];

		expect(getImagePreviewNavigation(mixedFiles, mixedFiles[3])).toEqual({
			previousFile: mixedFiles[1],
			nextFile: mixedFiles[4],
		});
	});

	it("returns no navigation when the current file is not in the image queue", () => {
		expect(getImagePreviewNavigation(files, files[1])).toEqual({});
		expect(
			getImagePreviewNavigation(files, {
				id: 99,
				mime_type: "image/png",
				name: "missing.png",
			}),
		).toEqual({});
	});

	it("returns no navigation without a current file or with only one image", () => {
		expect(getImagePreviewNavigation(files, null)).toEqual({});
		expect(getImagePreviewNavigation(files, undefined)).toEqual({});
		expect(getImagePreviewNavigation([files[0], files[1]], files[0])).toEqual(
			{},
		);
	});

	it("treats explicit image categories as images even when the extension is unknown", () => {
		const categorizedFiles = [
			{
				id: 10,
				file_category: "image",
				mime_type: "application/octet-stream",
				name: "camera-output.unknown",
			},
			{
				id: 11,
				file_category: "image",
				mime_type: "image/png",
				name: "browser.png",
			},
		];

		expect(
			getImagePreviewNavigation(categorizedFiles, categorizedFiles[0]),
		).toEqual({
			previousFile: categorizedFiles[1],
			nextFile: categorizedFiles[1],
		});
	});

	it("uses configured image-preview extensions for converted image formats", () => {
		const rawFiles = [
			{
				id: 7,
				mime_type: "application/octet-stream",
				name: "capture.nef",
			},
			{
				id: 8,
				mime_type: "image/png",
				name: "browser.png",
			},
		];

		expect(
			getImagePreviewNavigation(
				rawFiles,
				rawFiles[0],
				thumbnailSupport(["nef"]),
			),
		).toEqual({
			previousFile: rawFiles[1],
			nextFile: rawFiles[1],
		});
	});

	it("does not include converted formats without configured image-preview support", () => {
		const rawFiles = [
			{
				id: 7,
				mime_type: "application/octet-stream",
				name: "capture.nef",
			},
			{
				id: 8,
				mime_type: "image/png",
				name: "browser.png",
			},
		];

		expect(
			getImagePreviewNavigation(rawFiles, rawFiles[0], thumbnailSupport([])),
		).toEqual({});
	});
});
