import { describe, expect, it } from "vitest";
import {
	getMediaDataExtension,
	mediaDataKindForFile,
	supportsAudioMediaData,
	supportsMediaData,
} from "@/lib/mediaDataSupport";
import type { PublicMediaDataSupport } from "@/types/api";

const supportConfig: PublicMediaDataSupport = {
	enabled: true,
	kinds: {
		audio: {
			enabled: true,
			extensions: ["mp3", "flac"],
			match: "extensions",
		},
		image: {
			enabled: true,
			extensions: ["jpg"],
			match: "extensions",
		},
		video: {
			enabled: true,
			extensions: [],
			match: "any",
		},
	},
	max_source_bytes: 1024,
	version: 1,
};

describe("mediaDataSupport", () => {
	it("normalizes media data file extensions", () => {
		expect(getMediaDataExtension(" Song.MP3 ")).toBe("mp3");
		expect(getMediaDataExtension("archive.tar.gz")).toBe("gz");
		expect(getMediaDataExtension(".gitignore")).toBe("");
		expect(getMediaDataExtension("no-extension")).toBe("");
		expect(getMediaDataExtension("trailing.")).toBe("");
	});

	it("detects media metadata kinds from explicit categories before MIME types", () => {
		expect(
			mediaDataKindForFile({ file_category: "audio", mime_type: "text/plain" }),
		).toBe("audio");
		expect(mediaDataKindForFile({ mime_type: "image/jpeg" })).toBe("image");
		expect(mediaDataKindForFile({ mime_type: "audio/mpeg" })).toBe("audio");
		expect(mediaDataKindForFile({ mime_type: "video/mp4" })).toBe("video");
		expect(mediaDataKindForFile({ mime_type: "application/pdf" })).toBeNull();
	});

	it("checks configured media data support by kind, size, and extension", () => {
		expect(
			supportsMediaData(
				{ mime_type: "audio/mpeg", name: "Track.MP3", size: 1024 },
				supportConfig,
			),
		).toBe(true);
		expect(
			supportsMediaData(
				{ mime_type: "audio/mpeg", name: "Track.wav", size: 1024 },
				supportConfig,
			),
		).toBe(false);
		expect(
			supportsMediaData(
				{ mime_type: "audio/mpeg", name: "Track", size: 1024 },
				supportConfig,
			),
		).toBe(false);
		expect(
			supportsMediaData(
				{ mime_type: "audio/mpeg", name: "Track.mp3", size: 1025 },
				supportConfig,
			),
		).toBe(false);
		expect(
			supportsMediaData(
				{ mime_type: "audio/mpeg", name: "Track.mp3", size: 1024 },
				{
					...supportConfig,
					kinds: {
						...supportConfig.kinds,
						audio: {
							...supportConfig.kinds.audio,
							enabled: false,
						},
					},
				},
			),
		).toBe(false);
		expect(
			supportsMediaData(
				{ mime_type: "video/mp4", name: "clip.unknown", size: 1024 },
				supportConfig,
			),
		).toBe(true);
		expect(
			supportsMediaData(
				{ mime_type: "application/pdf", name: "Manual.pdf", size: 1 },
				supportConfig,
			),
		).toBe(false);
	});

	it("checks audio-specific media data support", () => {
		expect(
			supportsAudioMediaData(
				{ mime_type: "audio/flac", name: "track.flac" },
				supportConfig,
			),
		).toBe(true);
		expect(
			supportsAudioMediaData(
				{ mime_type: "video/mp4", name: "clip.mp4" },
				supportConfig,
			),
		).toBe(false);
		expect(
			supportsAudioMediaData(
				{ mime_type: "audio/mpeg", name: "track.mp3" },
				{ ...supportConfig, enabled: false },
			),
		).toBe(false);
	});
});
