import { derivedFileResource } from "@/lib/fileResource";
import { supportsAudioMediaData } from "@/lib/mediaDataSupport";
import { resourceRequestPath } from "@/lib/resourceRequest";
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

export async function buildDirectMusicTrack(
	file: MusicFileLike,
): Promise<MusicPlayerTrack> {
	const resource = await fileService.resolveResourceHandle(file.id, {
		delivery_mode: "direct_url",
		purpose: "preview",
		representation: "original",
	});
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
		path: resourceRequestPath(resource),
		resource,
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

export async function buildDirectMusicQueue(files: MusicFileLike[]) {
	const results = await Promise.allSettled(
		files.filter(isMusicFile).map((file) => buildDirectMusicTrack(file)),
	);
	return results.flatMap((result) =>
		result.status === "fulfilled" ? [result.value] : [],
	);
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
		resource: derivedFileResource(shareService.downloadPath(token), {
			deliveryMode: "direct_url",
			mimeType: file.mime_type,
			scope: "share",
		}),
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
		resource: derivedFileResource(
			shareService.downloadFolderPath(token, file.id),
			{
				deliveryMode: "direct_url",
				mimeType: file.mime_type,
				scope: "share",
			},
		),
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
	return files.flatMap((file) =>
		isMusicFile(file) ? [buildShareFolderMusicTrack(token, file)] : [],
	);
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
		resource: derivedFileResource(link.path, {
			deliveryMode: "direct_url",
			mimeType: track.mimeType,
			scope: "share",
		}),
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
