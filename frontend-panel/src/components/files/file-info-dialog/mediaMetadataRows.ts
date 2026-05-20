import type {
	AudioMediaMetadata,
	FileInfo,
	FileListItem,
	ImageMediaMetadata,
	MediaMetadataInfo,
	MediaMetadataKind,
	VideoMediaMetadata,
} from "@/types/api";
import type { DetailRow } from "./types";

type Translate = (key: string, options?: Record<string, unknown>) => string;

export function mediaMetadataKindForFile(
	file: FileInfo | FileListItem,
): MediaMetadataKind | null {
	if (
		file.file_category === "image" ||
		file.file_category === "audio" ||
		file.file_category === "video"
	) {
		return file.file_category;
	}
	if (file.mime_type.startsWith("image/")) return "image";
	if (file.mime_type.startsWith("audio/")) return "audio";
	if (file.mime_type.startsWith("video/")) return "video";
	return null;
}

function cleanInfoText(value: string | null | undefined) {
	const normalized = value?.trim();
	return normalized ? normalized : null;
}

function trimDecimal(value: number, digits = 1) {
	return Number.isInteger(value)
		? value.toString()
		: value.toFixed(digits).replace(/\.?0+$/, "");
}

function isPositiveFiniteNumber(
	value: number | null | undefined,
): value is number {
	return typeof value === "number" && Number.isFinite(value) && value > 0;
}

function formatFNumber(value: number | null | undefined) {
	if (!isPositiveFiniteNumber(value)) return null;
	return `ƒ/${trimDecimal(value, 1)}`;
}

function formatExposureSeconds(
	value: number | null | undefined,
	secondLabel: string,
) {
	if (!isPositiveFiniteNumber(value)) return null;
	if (value < 1) {
		const denominator = Math.round(1 / value);
		if (denominator > 0) {
			return `1/${denominator} ${secondLabel}`;
		}
	}
	return `${trimDecimal(value, 3)} ${secondLabel}`;
}

function formatExposureBias(value: number | null | undefined) {
	if (typeof value !== "number" || !Number.isFinite(value)) return null;
	const normalized = Object.is(value, -0) ? 0 : value;
	return `${normalized.toFixed(1)} ev`;
}

function formatFocalLength(value: number | null | undefined) {
	if (!isPositiveFiniteNumber(value)) return null;
	return `${trimDecimal(value, 1)}mm`;
}

function formatDurationMs(value: number | null | undefined) {
	if (!isPositiveFiniteNumber(value)) return null;

	const totalSeconds = Math.round(value / 1000);
	const hours = Math.floor(totalSeconds / 3600);
	const minutes = Math.floor((totalSeconds % 3600) / 60);
	const seconds = totalSeconds % 60;

	if (hours > 0) {
		return `${hours}:${minutes.toString().padStart(2, "0")}:${seconds
			.toString()
			.padStart(2, "0")}`;
	}
	return `${minutes}:${seconds.toString().padStart(2, "0")}`;
}

function formatSampleRate(value: number | null | undefined) {
	if (!isPositiveFiniteNumber(value)) return null;
	if (value >= 1000) {
		return `${trimDecimal(value / 1000, 1)} kHz`;
	}
	return `${value} Hz`;
}

function formatBitrateKbps(value: number | null | undefined) {
	if (!isPositiveFiniteNumber(value)) return null;
	return `${Math.round(value)} kbps`;
}

function formatBitrateBps(value: number | null | undefined) {
	if (!isPositiveFiniteNumber(value)) return null;
	if (value >= 1_000_000) {
		return `${trimDecimal(value / 1_000_000, 1)} Mbps`;
	}
	return `${Math.round(value / 1000)} kbps`;
}

function formatChannels(value: number | null | undefined, t: Translate) {
	if (!isPositiveFiniteNumber(value)) return null;
	if (value === 1) return t("info_media_channels_mono");
	if (value === 2) return t("info_media_channels_stereo");
	return `${value} ${t("info_media_channels_unit")}`;
}

function formatBitDepth(value: number | null | undefined) {
	if (!isPositiveFiniteNumber(value)) return null;
	return `${value}-bit`;
}

function formatNumberPair(
	value: number | null | undefined,
	total: number | null | undefined,
) {
	if (!isPositiveFiniteNumber(value)) return null;
	if (isPositiveFiniteNumber(total)) {
		return `${value}/${total}`;
	}
	return String(value);
}

function formatVideoResolution(metadata: VideoMediaMetadata, t: Translate) {
	const width = metadata.display_width ?? metadata.width;
	const height = metadata.display_height ?? metadata.height;
	if (!isPositiveFiniteNumber(width) || !isPositiveFiniteNumber(height)) {
		return null;
	}
	const orientation =
		width > height
			? "info_media_orientation_landscape"
			: height > width
				? "info_media_orientation_portrait"
				: null;
	return orientation
		? `${width} x ${height} · ${t(orientation)}`
		: `${width} x ${height}`;
}

function formatFrameRate(value: string | null | undefined) {
	const normalized = cleanInfoText(value);
	if (!normalized) return null;

	const ratioMatch = normalized.match(/^(\d+(?:\.\d+)?)\/(\d+(?:\.\d+)?)$/);
	if (ratioMatch) {
		const numerator = Number(ratioMatch[1]);
		const denominator = Number(ratioMatch[2]);
		if (Number.isFinite(numerator) && denominator > 0) {
			return `${trimDecimal(numerator / denominator, 2)} fps`;
		}
	}

	const numeric = Number(normalized);
	if (Number.isFinite(numeric) && numeric > 0) {
		return `${trimDecimal(numeric, 2)} fps`;
	}
	return normalized;
}

function formatCodecName(value: string | null | undefined) {
	const normalized = cleanInfoText(value);
	if (!normalized) return null;
	switch (normalized.toLowerCase()) {
		case "h264":
		case "avc1":
			return "H.264 / AVC";
		case "hevc":
		case "h265":
			return "H.265 / HEVC";
		case "av1":
			return "AV1";
		case "vp9":
			return "VP9";
		case "vp8":
			return "VP8";
		case "mpeg4":
			return "MPEG-4";
		case "prores":
			return "ProRes";
		case "aac":
			return "AAC";
		case "mp3":
			return "MP3";
		case "opus":
			return "Opus";
		case "vorbis":
			return "Vorbis";
		case "flac":
			return "FLAC";
		default:
			return normalized;
	}
}

function formatContainerName(value: string | null | undefined) {
	const normalized = cleanInfoText(value);
	if (!normalized) return null;
	const tokens = normalized
		.toLowerCase()
		.split(",")
		.map((token) => token.trim())
		.filter(Boolean);
	if (tokens.includes("matroska") && tokens.includes("webm")) {
		return "Matroska / WebM";
	}
	if (tokens.includes("matroska")) return "Matroska";
	if (tokens.includes("webm")) return "WebM";
	if (tokens.includes("mpegts")) return "MPEG-TS";
	if (tokens.includes("avi")) return "AVI";
	if (tokens.includes("mov") && tokens.includes("mp4")) {
		return "MP4 / QuickTime";
	}
	if (tokens.includes("mp4")) return "MP4";
	if (tokens.includes("mov")) return "QuickTime";
	return normalized;
}

function formatColorToken(value: string | null | undefined) {
	const normalized = cleanInfoText(value);
	if (!normalized) return null;
	switch (normalized.toLowerCase()) {
		case "bt2020":
		case "bt2020nc":
		case "bt2020c":
			return "BT.2020";
		case "bt709":
			return "BT.709";
		case "smpte2084":
			return "PQ";
		case "arib-std-b67":
			return "HLG";
		case "smpte170m":
			return "SMPTE 170M";
		default:
			return normalized;
	}
}

function pushUnique(parts: string[], value: string | null | undefined) {
	if (value && !parts.includes(value)) {
		parts.push(value);
	}
}

function formatVideoColor(metadata: VideoMediaMetadata) {
	const parts: string[] = [];
	pushUnique(parts, cleanInfoText(metadata.hdr_format));
	pushUnique(parts, formatBitDepth(metadata.bit_depth));
	pushUnique(parts, formatColorToken(metadata.color_primaries));
	pushUnique(parts, formatColorToken(metadata.color_space));
	pushUnique(parts, formatColorToken(metadata.color_transfer));
	if (parts.length === 0) {
		pushUnique(parts, cleanInfoText(metadata.pixel_format));
	}
	return parts.length > 0 ? parts.join(" · ") : null;
}

function formatVideoAudioSummary(metadata: VideoMediaMetadata, t: Translate) {
	const parts: string[] = [];
	if (metadata.audio_stream_count > 1) {
		parts.push(
			t("info_media_audio_tracks_count", {
				count: metadata.audio_stream_count,
			}),
		);
	}
	pushUnique(parts, formatCodecName(metadata.audio_codec));
	pushUnique(parts, formatChannels(metadata.audio_channels, t));
	pushUnique(parts, formatSampleRate(metadata.audio_sample_rate));
	pushUnique(parts, formatBitrateBps(metadata.audio_bitrate));
	return parts.length > 0 ? parts.join(" · ") : null;
}

function formatTakenAt(value: string | null | undefined) {
	const normalized = cleanInfoText(value);
	if (!normalized) return null;
	const match = normalized.match(
		/^(\d{4})-(\d{2})-(\d{2})[T ](\d{2}):(\d{2}):(\d{2})/,
	);
	if (!match) return normalized;
	return `${Number(match[1])}/${Number(match[2])}/${Number(match[3])} ${match[4]}:${match[5]}:${match[6]}`;
}

function formatImageResolution(metadata: ImageMediaMetadata) {
	const megapixels = (metadata.width * metadata.height) / 1_000_000;
	return `${megapixels.toFixed(1)} MP · ${metadata.width} x ${metadata.height}`;
}

function imageMediaMetadata(info: MediaMetadataInfo | null) {
	if (
		info?.status !== "ready" ||
		info.kind !== "image" ||
		info.metadata?.kind !== "image" ||
		typeof info.metadata.width !== "number" ||
		typeof info.metadata.height !== "number"
	) {
		return null;
	}
	return info.metadata;
}

function audioMediaMetadata(info: MediaMetadataInfo | null) {
	if (
		info?.status !== "ready" ||
		info.kind !== "audio" ||
		info.metadata?.kind !== "audio"
	) {
		return null;
	}
	return info.metadata;
}

function videoMediaMetadata(info: MediaMetadataInfo | null) {
	if (
		info?.status !== "ready" ||
		info.kind !== "video" ||
		info.metadata?.kind !== "video"
	) {
		return null;
	}
	return info.metadata;
}

function filterDetailRows(rows: DetailRow[]) {
	return rows.filter((row) => {
		if (row.value === null || row.value === undefined) return false;
		return typeof row.value !== "string" || row.value.trim().length > 0;
	});
}

function buildImageMetadataRows({
	loading,
	loadingText,
	metadata,
	t,
}: {
	loading: boolean;
	loadingText: string;
	metadata: ImageMediaMetadata | null;
	t: Translate;
}): DetailRow[] {
	if (!metadata) {
		return loading
			? [{ label: t("info_media_metadata_status"), value: loadingText }]
			: [];
	}

	const camera = [metadata.camera_make, metadata.camera_model]
		.map(cleanInfoText)
		.filter((value): value is string => value !== null)
		.join(" ");
	const focalLength = formatFocalLength(metadata.focal_length_mm);
	const lensModel = cleanInfoText(metadata.lens_model);
	const lens =
		focalLength && lensModel
			? `${focalLength} (${lensModel})`
			: (lensModel ?? focalLength);

	return filterDetailRows([
		{ label: t("info_exif_aperture"), value: formatFNumber(metadata.f_number) },
		{
			label: t("info_exif_exposure"),
			value: formatExposureSeconds(
				metadata.exposure_time_seconds,
				t("info_exif_seconds"),
			),
		},
		{
			label: t("info_exif_iso"),
			value: Number.isFinite(metadata.iso) ? String(metadata.iso) : null,
		},
		{
			label: t("info_exif_exposure_bias"),
			value: formatExposureBias(metadata.exposure_bias_ev),
		},
		{
			label: t("info_exif_flash"),
			value:
				typeof metadata.flash_fired === "boolean"
					? metadata.flash_fired
						? t("info_exif_flash_on")
						: t("info_exif_flash_off")
					: null,
		},
		{
			label: t("info_exif_camera"),
			value: camera || null,
		},
		{ label: t("info_exif_lens"), value: lens },
		{ label: t("info_exif_taken_at"), value: formatTakenAt(metadata.taken_at) },
		{
			label: t("info_exif_resolution"),
			value: formatImageResolution(metadata),
		},
		{ label: t("info_exif_author"), value: cleanInfoText(metadata.artist) },
		{ label: t("info_exif_software"), value: cleanInfoText(metadata.software) },
	]);
}

function buildAudioMetadataRows({
	loading,
	loadingText,
	metadata,
	t,
}: {
	loading: boolean;
	loadingText: string;
	metadata: AudioMediaMetadata | null;
	t: Translate;
}): DetailRow[] {
	if (!metadata) {
		return loading
			? [{ label: t("info_media_metadata_status"), value: loadingText }]
			: [];
	}

	const artists = metadata.artists
		.map(cleanInfoText)
		.filter((value): value is string => value !== null);
	const artist =
		artists.length > 0 ? artists.join(", ") : cleanInfoText(metadata.artist);

	return filterDetailRows([
		{ label: t("info_media_title"), value: cleanInfoText(metadata.title) },
		{ label: t("info_media_artist"), value: artist },
		{ label: t("info_media_album"), value: cleanInfoText(metadata.album) },
		{
			label: t("info_media_album_artist"),
			value: cleanInfoText(metadata.album_artist),
		},
		{
			label: t("info_media_duration"),
			value: formatDurationMs(metadata.duration_ms),
		},
		{
			label: t("info_media_sample_rate"),
			value: formatSampleRate(metadata.sample_rate),
		},
		{
			label: t("info_media_channels"),
			value: formatChannels(metadata.channels, t),
		},
		{
			label: t("info_media_bit_depth"),
			value: formatBitDepth(metadata.bit_depth),
		},
		{
			label: t("info_media_audio_bitrate"),
			value: formatBitrateKbps(metadata.audio_bitrate),
		},
		{
			label: t("info_media_overall_bitrate"),
			value: formatBitrateKbps(metadata.overall_bitrate),
		},
		{
			label: t("info_media_track"),
			value: formatNumberPair(metadata.track_number, metadata.track_total),
		},
		{
			label: t("info_media_disc"),
			value: formatNumberPair(metadata.disc_number, metadata.disc_total),
		},
		{ label: t("info_media_genre"), value: cleanInfoText(metadata.genre) },
		{ label: t("info_media_date"), value: cleanInfoText(metadata.date) },
		{
			label: t("info_media_embedded_cover"),
			value: metadata.has_embedded_picture
				? (cleanInfoText(metadata.embedded_picture_mime_type) ??
					t("info_media_embedded_cover_yes"))
				: t("info_media_embedded_cover_no"),
		},
	]);
}

function buildVideoMetadataRows({
	loading,
	loadingText,
	metadata,
	t,
}: {
	loading: boolean;
	loadingText: string;
	metadata: VideoMediaMetadata | null;
	t: Translate;
}): DetailRow[] {
	if (!metadata) {
		return loading
			? [{ label: t("info_media_metadata_status"), value: loadingText }]
			: [];
	}

	return filterDetailRows([
		{
			label: t("info_media_duration"),
			value: formatDurationMs(metadata.duration_ms),
		},
		{
			label: t("info_media_resolution"),
			value: formatVideoResolution(metadata, t),
		},
		{
			label: t("info_media_codec"),
			value: formatCodecName(metadata.codec),
		},
		{
			label: t("info_media_frame_rate"),
			value: formatFrameRate(metadata.frame_rate),
		},
		{
			label: t("info_media_video_bitrate"),
			value: formatBitrateBps(metadata.video_bitrate),
		},
		{
			label: t("info_media_overall_bitrate"),
			value: formatBitrateBps(metadata.overall_bitrate),
		},
		{
			label: t("info_media_color"),
			value: formatVideoColor(metadata),
		},
		{
			label: t("info_media_audio"),
			value: formatVideoAudioSummary(metadata, t),
		},
		{
			label: t("info_media_subtitles"),
			value:
				metadata.subtitle_stream_count > 0
					? t("info_media_subtitle_tracks_count", {
							count: metadata.subtitle_stream_count,
						})
					: null,
		},
		{
			label: t("info_media_created_at"),
			value: formatTakenAt(metadata.creation_time),
		},
		{
			label: t("info_media_container"),
			value: formatContainerName(metadata.container),
		},
	]);
}

export function buildMediaMetadataRows({
	kind,
	loading,
	loadingText,
	metadata,
	t,
}: {
	kind: MediaMetadataKind;
	loading: boolean;
	loadingText: string;
	metadata: MediaMetadataInfo | null;
	t: Translate;
}) {
	switch (kind) {
		case "image":
			return buildImageMetadataRows({
				loading,
				loadingText,
				metadata: imageMediaMetadata(metadata),
				t,
			});
		case "audio":
			return buildAudioMetadataRows({
				loading,
				loadingText,
				metadata: audioMediaMetadata(metadata),
				t,
			});
		case "video":
			return buildVideoMetadataRows({
				loading,
				loadingText,
				metadata: videoMediaMetadata(metadata),
				t,
			});
	}
}
