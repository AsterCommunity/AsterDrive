import { describe, expect, it } from "vitest";
import {
	buildMediaMetadataRows,
	mediaMetadataKindForFile,
} from "@/components/files/file-info-dialog/mediaMetadataRows";
import type { DetailRow } from "@/components/files/file-info-dialog/types";
import type { MediaMetadataInfo } from "@/types/api";

const t = (key: string, options?: Record<string, unknown>) => {
	if (key === "info_media_channels_count") {
		return `channels:${options?.count}`;
	}
	if (key === "info_media_audio_tracks_count") {
		return `audio-tracks:${options?.count}`;
	}
	if (key === "info_media_subtitle_tracks_count") {
		return `subtitles:${options?.count}`;
	}
	return key;
};

function valuesByLabel(rows: DetailRow[]) {
	return new Map(rows.map((row) => [row.label, row.value]));
}

describe("mediaMetadataRows", () => {
	it("detects media metadata kind from persisted category or MIME type", () => {
		expect(
			mediaMetadataKindForFile({
				file_category: "audio",
				id: 1,
				mime_type: "application/octet-stream",
				name: "track.bin",
			} as never),
		).toBe("audio");
		expect(
			mediaMetadataKindForFile({
				file_category: "other",
				id: 2,
				mime_type: "IMAGE/PNG",
				name: "photo",
			} as never),
		).toBe("image");
		expect(
			mediaMetadataKindForFile({
				file_category: "document",
				id: 3,
				mime_type: "video/mp4",
				name: "clip.mp4",
			} as never),
		).toBe("video");
		expect(
			mediaMetadataKindForFile({
				file_category: "document",
				id: 4,
				mime_type: "application/pdf",
				name: "manual.pdf",
			} as never),
		).toBeNull();
	});

	it("shows loading rows for missing or incompatible metadata only while loading", () => {
		expect(
			buildMediaMetadataRows({
				kind: "image",
				loading: true,
				loadingText: "loading",
				metadata: null,
				t,
			}),
		).toEqual([{ label: "info_media_metadata_status", value: "loading" }]);
		expect(
			buildMediaMetadataRows({
				kind: "image",
				loading: false,
				loadingText: "loading",
				metadata: null,
				t,
			}),
		).toEqual([]);
		expect(
			buildMediaMetadataRows({
				kind: "audio",
				loading: true,
				loadingText: "loading",
				metadata: {
					kind: "image",
					metadata: { kind: "image", width: 1, height: 1 },
					status: "ready",
				} as MediaMetadataInfo,
				t,
			}),
		).toEqual([{ label: "info_media_metadata_status", value: "loading" }]);
	});

	it("formats image metadata and filters empty EXIF fields", () => {
		const rows = valuesByLabel(
			buildMediaMetadataRows({
				kind: "image",
				loading: false,
				loadingText: "loading",
				metadata: {
					kind: "image",
					metadata: {
						artist: "  Casey  ",
						camera_make: " Canon ",
						camera_model: " R5 ",
						exposure_bias_ev: -0,
						exposure_time_seconds: 2.5,
						f_number: 4,
						flash_fired: true,
						focal_length_mm: 35.5,
						format: "image/jpeg",
						gps_altitude_meters: 12.34,
						gps_latitude: 36,
						gps_longitude: 120.5,
						height: 3000,
						iso: Number.NaN,
						kind: "image",
						lens_model: "  ",
						software: "",
						taken_at: "2024:04:01 05:44:11",
						width: 4000,
					},
					status: "ready",
				} as MediaMetadataInfo,
				t,
			}),
		);

		expect(rows.get("info_exif_aperture")).toBe("ƒ/4");
		expect(rows.get("info_exif_exposure")).toBe("2.5 info_exif_seconds");
		expect(rows.get("info_exif_iso")).toBeUndefined();
		expect(rows.get("info_exif_exposure_bias")).toBe("0.0 ev");
		expect(rows.get("info_exif_flash")).toBe("info_exif_flash_on");
		expect(rows.get("info_exif_camera")).toBe("Canon R5");
		expect(rows.get("info_exif_lens")).toBe("35.5mm");
		expect(rows.get("info_exif_taken_at")).toBe("2024:04:01 05:44:11");
		expect(rows.get("info_exif_resolution")).toBe("12.0 MP · 4000 x 3000");
		expect(rows.get("info_exif_location")).toBe(
			"36.000000 · 120.500000 · 12.3 m",
		);
		expect(rows.get("info_exif_author")).toBe("Casey");
		expect(rows.get("info_exif_software")).toBeUndefined();
	});

	it("formats audio metadata variants", () => {
		const rows = valuesByLabel(
			buildMediaMetadataRows({
				kind: "audio",
				loading: false,
				loadingText: "loading",
				metadata: {
					kind: "audio",
					metadata: {
						album: "  Album ",
						album_artist: " ",
						artist: " Solo Artist ",
						artists: [" ", "Solo Artist"],
						audio_bitrate: 256.4,
						bit_depth: 24,
						channels: 6,
						date: "2026",
						disc_number: 2,
						disc_total: null,
						duration_ms: 3_661_000,
						embedded_picture_mime_type: "",
						genre: "Electronic",
						has_embedded_picture: true,
						kind: "audio",
						overall_bitrate: 999.6,
						sample_rate: 960,
						title: " Title ",
						track_number: 7,
						track_total: null,
					},
					status: "ready",
				} as MediaMetadataInfo,
				t,
			}),
		);

		expect(rows.get("info_media_title")).toBe("Title");
		expect(rows.get("info_media_artist")).toBe("Solo Artist");
		expect(rows.get("info_media_album")).toBe("Album");
		expect(rows.get("info_media_album_artist")).toBeUndefined();
		expect(rows.get("info_media_duration")).toBe("1:01:01");
		expect(rows.get("info_media_sample_rate")).toBe("960 Hz");
		expect(rows.get("info_media_channels")).toBe("channels:6");
		expect(rows.get("info_media_bit_depth")).toBe("24-bit");
		expect(rows.get("info_media_audio_bitrate")).toBe("256 kbps");
		expect(rows.get("info_media_overall_bitrate")).toBe("1000 kbps");
		expect(rows.get("info_media_track")).toBe("7");
		expect(rows.get("info_media_disc")).toBe("2");
		expect(rows.get("info_media_embedded_cover")).toBe(
			"info_media_embedded_cover_yes",
		);
	});

	it("formats video metadata variants", () => {
		const rows = valuesByLabel(
			buildMediaMetadataRows({
				kind: "video",
				loading: false,
				loadingText: "loading",
				metadata: {
					kind: "video",
					metadata: {
						audio_bitrate: 96_000,
						audio_channels: 1,
						audio_codec: "opus",
						audio_sample_rate: 44_100,
						audio_stream_count: 3,
						bit_depth: null,
						codec: "hevc",
						color_primaries: null,
						color_space: null,
						color_transfer: null,
						container: "matroska,webm",
						creation_time: "2024-04-01 05:44:11",
						display_height: 1080,
						display_width: 1080,
						duration_ms: 61_000,
						frame_rate: "24",
						height: 1080,
						hdr_format: null,
						kind: "video",
						overall_bitrate: 950_000,
						pixel_format: "yuv420p",
						rotation_degrees: 0,
						subtitle_stream_count: 0,
						video_bitrate: 123_456,
						width: 1080,
					},
					status: "ready",
				} as MediaMetadataInfo,
				t,
			}),
		);

		expect(rows.get("info_media_duration")).toBe("1:01");
		expect(rows.get("info_media_resolution")).toBe("1080 x 1080");
		expect(rows.get("info_media_codec")).toBe("H.265 / HEVC");
		expect(rows.get("info_media_frame_rate")).toBe("24 fps");
		expect(rows.get("info_media_video_bitrate")).toBe("123 kbps");
		expect(rows.get("info_media_overall_bitrate")).toBe("950 kbps");
		expect(rows.get("info_media_color")).toBe("yuv420p");
		expect(rows.get("info_media_audio")).toBe(
			"audio-tracks:3 · Opus · info_media_channels_mono · 44.1 kHz · 96 kbps",
		);
		expect(rows.get("info_media_subtitles")).toBeUndefined();
		expect(rows.get("info_media_created_at")).toBe("2024/4/1 05:44:11");
		expect(rows.get("info_media_container")).toBe("Matroska / WebM");
	});

	it("handles sparse video metadata and preserves unknown tokens", () => {
		const rows = valuesByLabel(
			buildMediaMetadataRows({
				kind: "video",
				loading: false,
				loadingText: "loading",
				metadata: {
					kind: "video",
					metadata: {
						audio_bitrate: null,
						audio_channels: null,
						audio_codec: null,
						audio_sample_rate: null,
						audio_stream_count: 0,
						bit_depth: 8,
						codec: "custom-codec",
						color_primaries: "bt709",
						color_space: "smpte170m",
						color_transfer: "arib-std-b67",
						container: "avi",
						creation_time: "not-a-date",
						display_height: null,
						display_width: null,
						duration_ms: 0,
						frame_rate: "not-a-rate",
						height: 480,
						hdr_format: "BT.709",
						kind: "video",
						overall_bitrate: null,
						pixel_format: "unused",
						rotation_degrees: 0,
						subtitle_stream_count: 1,
						video_bitrate: null,
						width: 640,
					},
					status: "ready",
				} as MediaMetadataInfo,
				t,
			}),
		);

		expect(rows.get("info_media_duration")).toBeUndefined();
		expect(rows.get("info_media_resolution")).toBe(
			"640 x 480 · info_media_orientation_landscape",
		);
		expect(rows.get("info_media_codec")).toBe("custom-codec");
		expect(rows.get("info_media_frame_rate")).toBe("not-a-rate");
		expect(rows.get("info_media_color")).toBe(
			"BT.709 · 8-bit · SMPTE 170M · HLG",
		);
		expect(rows.get("info_media_audio")).toBeUndefined();
		expect(rows.get("info_media_subtitles")).toBe("subtitles:1");
		expect(rows.get("info_media_created_at")).toBe("not-a-date");
		expect(rows.get("info_media_container")).toBe("AVI");
	});

	it("covers common video codec and container labels", () => {
		const cases = [
			{
				codec: "av1",
				container: "mpegts",
				expectedCodec: "AV1",
				expectedContainer: "MPEG-TS",
			},
			{
				codec: "vp9",
				container: "webm",
				expectedCodec: "VP9",
				expectedContainer: "WebM",
			},
			{
				codec: "vp8",
				container: "mp4",
				expectedCodec: "VP8",
				expectedContainer: "MP4",
			},
			{
				codec: "mpeg4",
				container: "mov",
				expectedCodec: "MPEG-4",
				expectedContainer: "QuickTime",
			},
			{
				codec: "prores",
				container: "custom-container",
				expectedCodec: "ProRes",
				expectedContainer: "custom-container",
			},
		];

		for (const entry of cases) {
			const rows = valuesByLabel(
				buildMediaMetadataRows({
					kind: "video",
					loading: false,
					loadingText: "loading",
					metadata: {
						kind: "video",
						metadata: {
							audio_bitrate: null,
							audio_channels: null,
							audio_codec: null,
							audio_sample_rate: null,
							audio_stream_count: 0,
							bit_depth: null,
							codec: entry.codec,
							color_primaries: null,
							color_space: null,
							color_transfer: null,
							container: entry.container,
							creation_time: null,
							display_height: null,
							display_width: null,
							duration_ms: null,
							frame_rate: null,
							height: 0,
							hdr_format: null,
							kind: "video",
							overall_bitrate: null,
							pixel_format: null,
							rotation_degrees: 0,
							subtitle_stream_count: 0,
							video_bitrate: null,
							width: 0,
						},
						status: "ready",
					} as MediaMetadataInfo,
					t,
				}),
			);

			expect(rows.get("info_media_codec")).toBe(entry.expectedCodec);
			expect(rows.get("info_media_container")).toBe(entry.expectedContainer);
		}
	});

	it("formats audio codec labels in video audio summaries", () => {
		const cases = [
			["mp3", "MP3"],
			["vorbis", "Vorbis"],
			["flac", "FLAC"],
		] as const;

		for (const [codec, expected] of cases) {
			const rows = valuesByLabel(
				buildMediaMetadataRows({
					kind: "video",
					loading: false,
					loadingText: "loading",
					metadata: {
						kind: "video",
						metadata: {
							audio_bitrate: null,
							audio_channels: null,
							audio_codec: codec,
							audio_sample_rate: null,
							audio_stream_count: 1,
							bit_depth: null,
							codec: null,
							color_primaries: null,
							color_space: null,
							color_transfer: null,
							container: null,
							creation_time: null,
							display_height: null,
							display_width: null,
							duration_ms: null,
							frame_rate: null,
							height: 0,
							hdr_format: null,
							kind: "video",
							overall_bitrate: null,
							pixel_format: null,
							rotation_degrees: 0,
							subtitle_stream_count: 0,
							video_bitrate: null,
							width: 0,
						},
						status: "ready",
					} as MediaMetadataInfo,
					t,
				}),
			);

			expect(rows.get("info_media_audio")).toBe(expected);
		}
	});
});
