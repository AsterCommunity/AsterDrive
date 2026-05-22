import { supportsAudioMediaData } from "@/lib/mediaDataSupport";
import { fileService } from "@/services/fileService";
import { shareService } from "@/services/shareService";
import { useMediaDataSupportStore } from "@/stores/mediaDataSupportStore";
import type {
	MusicPlayerTrack,
	MusicTrackMetadata,
} from "@/stores/musicPlayerStore";
import type {
	FileInfo,
	FileListItem,
	MediaMetadataInfo,
	MediaMetadataPayload,
	SharePublicInfo,
	ShareStreamSessionInfo,
} from "@/types/api";

const MUSIC_METADATA_FETCH_LIMIT_BYTES = 3 * 1024 * 1024;

export type MusicFileLike = Pick<
	FileInfo | FileListItem,
	"id" | "mime_type" | "name"
> & {
	file_category?: string;
	size?: number;
};

export function isMusicFile(file: MusicFileLike) {
	return file.file_category === "audio" || file.mime_type.startsWith("audio/");
}

function stripKnownAudioExtension(name: string) {
	return name.replace(
		/\.(mp3|wav|flac|aac|m4a|ogg|oga|opus|wma|aiff|aif|alac|mid|midi)$/i,
		"",
	);
}

function cleanMetadataText(value: string | null | undefined) {
	const normalized = value?.trim();
	return normalized ? normalized : null;
}

function cleanMetadataTexts(values: string[] | null | undefined) {
	const normalized = values
		?.map((value) => cleanMetadataText(value))
		.filter((value): value is string => value !== null);
	return normalized && normalized.length > 0 ? normalized : null;
}

function audioMetadataPayload(
	info: MediaMetadataInfo,
): Extract<MediaMetadataPayload, { kind: "audio" }> | null {
	if (info.kind !== "audio" || info.status !== "ready") {
		return null;
	}
	const metadata = info.metadata;
	return metadata?.kind === "audio" ? metadata : null;
}

export function backendAudioMetadataToTrackMetadata(
	info: MediaMetadataInfo,
): MusicTrackMetadata | null {
	const metadata = audioMetadataPayload(info);
	if (!metadata) return null;

	const artists =
		cleanMetadataTexts(metadata.artists) ??
		(cleanMetadataText(metadata.artist)
			? [cleanMetadataText(metadata.artist) as string]
			: null);
	const title = cleanMetadataText(metadata.title);
	const album = cleanMetadataText(metadata.album);

	if (!title && !album && (!artists || artists.length === 0)) {
		return null;
	}

	const result: MusicTrackMetadata = {};
	if (album) result.album = album;
	if (artists && artists.length > 0) {
		result.artists = artists;
		result.artist = cleanMetadataText(metadata.artist) ?? artists.join(", ");
	} else {
		const artist = cleanMetadataText(metadata.artist);
		if (artist) result.artist = artist;
	}
	if (title) result.title = title;
	return result;
}

export function inferMusicMetadata(file: MusicFileLike): MusicTrackMetadata {
	const baseName = stripKnownAudioExtension(file.name).trim() || file.name;
	const separatorMatch = baseName.match(/^(.+?)\s[-–—]\s(.+)$/);

	if (separatorMatch) {
		const artist = separatorMatch[1]?.trim() || null;
		return {
			artist,
			artists: artist ? [artist] : null,
			title: separatorMatch[2]?.trim() || baseName,
		};
	}

	return {
		artist: null,
		artists: null,
		title: baseName,
	};
}

function audioBackendMetadataLoader(
	file: MusicFileLike,
	loader: NonNullable<MusicPlayerTrack["loadBackendMetadata"]>,
): MusicPlayerTrack["loadBackendMetadata"] {
	return (signal) => {
		const support = useMediaDataSupportStore.getState().config;
		if (!supportsAudioMediaData(file, support)) return Promise.resolve(null);
		return loader(signal);
	};
}

export function buildDirectMusicTrack(file: MusicFileLike): MusicPlayerTrack {
	return {
		id: `file:${file.id}`,
		loadBackendMetadata: audioBackendMetadataLoader(file, async (signal) =>
			backendAudioMetadataToTrackMetadata(
				await fileService.getMediaMetadata(file.id, { signal }),
			),
		),
		metadata: inferMusicMetadata(file),
		mimeType: file.mime_type,
		name: file.name,
		path: fileService.downloadPath(file.id),
		size: file.size,
		thumbnail: {
			file: {
				file_category: "audio",
				id: file.id,
				mime_type: file.mime_type,
				name: file.name,
			},
			path: fileService.thumbnailPath(file.id),
		},
	};
}

export function buildDirectMusicQueue(files: MusicFileLike[]) {
	return files.filter(isMusicFile).map(buildDirectMusicTrack);
}

export function buildSingleShareMusicTrack(
	info: SharePublicInfo,
	token: string,
): MusicPlayerTrack | null {
	if (!info.mime_type || typeof info.size !== "number") return null;
	const file = {
		id: -1,
		mime_type: info.mime_type,
		name: info.name,
		size: info.size,
	};
	if (!isMusicFile(file)) return null;

	return {
		id: `share:${token}:file`,
		loadBackendMetadata: audioBackendMetadataLoader(file, async (signal) =>
			backendAudioMetadataToTrackMetadata(
				await shareService.getMediaMetadata(token, { signal }),
			),
		),
		metadata: inferMusicMetadata(file),
		mimeType: file.mime_type,
		name: file.name,
		path: shareService.downloadPath(token),
		refreshStreamLink: () => shareService.createStreamSession(token),
		size: file.size,
		thumbnail: {
			file: {
				file_category: "audio",
				id: -1,
				mime_type: file.mime_type,
				name: file.name,
			},
			path: shareService.thumbnailPath(token),
		},
	};
}

export function buildShareFolderMusicTrack(
	token: string,
	file: MusicFileLike,
): MusicPlayerTrack {
	return {
		id: `share:${token}:file:${file.id}`,
		loadBackendMetadata: audioBackendMetadataLoader(file, async (signal) =>
			backendAudioMetadataToTrackMetadata(
				await shareService.getFolderFileMediaMetadata(token, file.id, {
					signal,
				}),
			),
		),
		metadata: inferMusicMetadata(file),
		mimeType: file.mime_type,
		name: file.name,
		path: shareService.downloadFolderPath(token, file.id),
		refreshStreamLink: () =>
			shareService.createFolderFileStreamSession(token, file.id),
		size: file.size,
		thumbnail: {
			file: {
				file_category: "audio",
				id: file.id,
				mime_type: file.mime_type,
				name: file.name,
			},
			path: shareService.folderFileThumbnailPath(token, file.id),
		},
	};
}

export function buildShareFolderMusicQueue(
	token: string,
	files: MusicFileLike[],
) {
	return files
		.filter(isMusicFile)
		.map((file) => buildShareFolderMusicTrack(token, file));
}

export async function hydrateMusicTrackStreamLink(track: MusicPlayerTrack) {
	if (!track.refreshStreamLink) {
		return track;
	}

	const link: ShareStreamSessionInfo = await track.refreshStreamLink();
	return {
		...track,
		expiresAt: link.expires_at || undefined,
		path: link.path,
	};
}

export async function hydrateMusicQueueForPlayback(
	tracks: MusicPlayerTrack[],
	activeTrackId: string,
) {
	const activeTrack = tracks.find((track) => track.id === activeTrackId);
	if (!activeTrack) {
		return tracks;
	}

	const hydratedTrack = await hydrateMusicTrackStreamLink(activeTrack);
	return tracks.map((track) =>
		track.id === activeTrackId ? hydratedTrack : track,
	);
}

function pictureToDataUrl(picture: {
	data: Uint8Array;
	format?: string | null;
}) {
	const mimeType = picture.format?.trim() || "image/jpeg";
	const chunkSize = 0x8000;
	let binary = "";
	for (let index = 0; index < picture.data.length; index += chunkSize) {
		binary += String.fromCharCode(
			...picture.data.subarray(index, index + chunkSize),
		);
	}

	return `data:${mimeType};base64,${btoa(binary)}`;
}

function responseHasBoundedMusicMetadataBody(response: Response) {
	if (response.status === 206) {
		return true;
	}

	const contentRange = response.headers.get("Content-Range");
	if (contentRange) {
		return true;
	}

	const contentLength = Number(response.headers.get("Content-Length"));
	return (
		Number.isFinite(contentLength) &&
		contentLength > 0 &&
		contentLength <= MUSIC_METADATA_FETCH_LIMIT_BYTES
	);
}

export async function parseMusicMetadataFromSource({
	fallbackMetadata,
	mimeType,
	name,
	size,
	source,
	signal,
}: {
	fallbackMetadata?: MusicTrackMetadata;
	mimeType: string;
	name: string;
	size?: number;
	signal?: AbortSignal;
	source: string;
}): Promise<MusicTrackMetadata> {
	const headers = new Headers();
	if (!source.startsWith("blob:")) {
		headers.set("Range", `bytes=0-${MUSIC_METADATA_FETCH_LIMIT_BYTES - 1}`);
	}

	const response = await fetch(source, {
		credentials: "include",
		headers,
		signal,
	});
	if (!response.ok) {
		throw new Error(`music metadata request failed with ${response.status}`);
	}
	if (
		!source.startsWith("blob:") &&
		!responseHasBoundedMusicMetadataBody(response)
	) {
		return (
			fallbackMetadata ??
			inferMusicMetadata({ id: -1, mime_type: mimeType, name, size })
		);
	}

	const blob = await response.blob();
	const { parseBlob, selectCover } = await import("music-metadata");
	const parsed = await parseBlob(blob, {
		duration: false,
		skipCovers: false,
		skipPostHeaders: true,
	});
	const cover = selectCover(parsed.common.picture);
	const parsedArtists =
		cleanMetadataTexts(parsed.common.artists) ??
		(cleanMetadataText(parsed.common.artist)
			? [cleanMetadataText(parsed.common.artist) as string]
			: null);
	const fallbackArtists =
		fallbackMetadata?.artists ??
		(fallbackMetadata?.artist ? [fallbackMetadata.artist] : null);

	return {
		album: cleanMetadataText(parsed.common.album) ?? fallbackMetadata?.album,
		artist: parsedArtists?.join(", ") ?? fallbackMetadata?.artist,
		artists: parsedArtists ?? fallbackArtists,
		artworkUrl: cover
			? pictureToDataUrl(cover)
			: (fallbackMetadata?.artworkUrl ?? null),
		title:
			cleanMetadataText(parsed.common.title) ??
			fallbackMetadata?.title ??
			inferMusicMetadata({ id: -1, mime_type: mimeType, name, size }).title,
	};
}
