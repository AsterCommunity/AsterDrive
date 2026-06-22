import { beforeEach, describe, expect, it, vi } from "vitest";
import {
	backendAudioMetadataToTrackMetadata,
	buildDirectMusicQueue,
	buildShareFolderMusicQueue,
	buildSingleShareMusicTrack,
	hydrateMusicQueueForPlayback,
	hydrateMusicTrackStreamLink,
	inferMusicMetadata,
	isMusicFile,
} from "@/lib/musicPlayer";

const mockState = vi.hoisted(() => ({
	createFolderFileStreamSession: vi.fn(),
	createStreamSession: vi.fn(),
	downloadFolderPath: vi.fn(
		(token: string, fileId: number) => `/s/${token}/files/${fileId}/download`,
	),
	downloadPath: vi.fn((idOrToken: number | string) =>
		typeof idOrToken === "number"
			? `/files/${idOrToken}/download`
			: `/s/${idOrToken}/download`,
	),
	getFileMediaMetadata: vi.fn(),
	getShareFolderFileMediaMetadata: vi.fn(),
	getShareMediaMetadata: vi.fn(),
	mediaDataSupportStore: {
		config: {
			enabled: true,
			kinds: {
				audio: {
					enabled: true,
					extensions: ["mp3", "flac"],
					match: "extensions",
				},
				image: { enabled: true, extensions: ["jpg"], match: "extensions" },
				video: { enabled: false, extensions: [], match: "extensions" },
			},
			max_source_bytes: 1024 * 1024 * 1024,
			version: 1,
		},
	},
	thumbnailPath: vi.fn((idOrToken: number | string) =>
		typeof idOrToken === "number"
			? `/files/${idOrToken}/thumbnail`
			: `/s/${idOrToken}/thumbnail`,
	),
	folderFileThumbnailPath: vi.fn(
		(token: string, fileId: number) => `/s/${token}/files/${fileId}/thumbnail`,
	),
	resolveResourceHandle: vi.fn((id: number) =>
		Promise.resolve({
			kind: "ready",
			identity: {
				cacheKey: `/files/${id}/download`,
				etag: null,
				scope: "personal",
			},
			request: {
				url: `/files/${id}/download?disposition=inline`,
				credentials: "include",
				conditionalHeaders: "allowed",
				redirectPolicy: "same_origin_only",
			},
			delivery: {
				mode: "direct_url",
				mimeType: "audio/mpeg",
			},
		}),
	),
}));

vi.mock("@/stores/mediaDataSupportStore", () => ({
	useMediaDataSupportStore: {
		getState: () => mockState.mediaDataSupportStore,
	},
}));

vi.mock("@/services/fileService", () => ({
	fileService: {
		downloadPath: (id: number) => mockState.downloadPath(id),
		getMediaMetadata: (...args: unknown[]) =>
			mockState.getFileMediaMetadata(...args),
		resolveResourceHandle: (...args: unknown[]) =>
			mockState.resolveResourceHandle(...args),
		thumbnailPath: (id: number) => mockState.thumbnailPath(id),
	},
}));

vi.mock("@/services/shareService", () => ({
	shareService: {
		createFolderFileStreamSession: (...args: unknown[]) =>
			mockState.createFolderFileStreamSession(...args),
		createStreamSession: (...args: unknown[]) =>
			mockState.createStreamSession(...args),
		downloadFolderPath: (...args: unknown[]) =>
			mockState.downloadFolderPath(...args),
		downloadPath: (token: string) => mockState.downloadPath(token),
		folderFileThumbnailPath: (...args: unknown[]) =>
			mockState.folderFileThumbnailPath(...args),
		getFolderFileMediaMetadata: (...args: unknown[]) =>
			mockState.getShareFolderFileMediaMetadata(...args),
		getMediaMetadata: (...args: unknown[]) =>
			mockState.getShareMediaMetadata(...args),
		thumbnailPath: (token: string) => mockState.thumbnailPath(token),
	},
}));

describe("musicPlayer helpers", () => {
	beforeEach(() => {
		mockState.createFolderFileStreamSession.mockReset();
		mockState.createStreamSession.mockReset();
		mockState.downloadFolderPath.mockClear();
		mockState.downloadPath.mockClear();
		mockState.folderFileThumbnailPath.mockClear();
		mockState.getFileMediaMetadata.mockReset();
		mockState.getShareFolderFileMediaMetadata.mockReset();
		mockState.getShareMediaMetadata.mockReset();
		mockState.resolveResourceHandle.mockClear();
		mockState.mediaDataSupportStore.config = {
			enabled: true,
			kinds: {
				audio: {
					enabled: true,
					extensions: ["mp3", "flac"],
					match: "extensions",
				},
				image: { enabled: true, extensions: ["jpg"], match: "extensions" },
				video: { enabled: false, extensions: [], match: "extensions" },
			},
			max_source_bytes: 1024 * 1024 * 1024,
			version: 1,
		};
		mockState.thumbnailPath.mockClear();
	});

	it("recognizes music files by persisted category or MIME type", () => {
		expect(
			isMusicFile({
				file_category: "audio",
				id: 1,
				mime_type: "application/octet-stream",
				name: "track.bin",
				size: 1,
			}),
		).toBe(true);
		expect(
			isMusicFile({
				file_category: "other",
				id: 2,
				mime_type: "audio/flac",
				name: "track.flac",
				size: 1,
			}),
		).toBe(true);
		expect(
			isMusicFile({
				file_category: "document",
				id: 3,
				mime_type: "application/pdf",
				name: "manual.pdf",
				size: 1,
			}),
		).toBe(false);
	});

	it("infers title and artist from common file names", () => {
		expect(
			inferMusicMetadata({
				id: 1,
				mime_type: "audio/mpeg",
				name: "Artist - Song Name.mp3",
				size: 1,
			}),
		).toEqual({
			artist: "Artist",
			artists: ["Artist"],
			title: "Song Name",
		});
		expect(
			inferMusicMetadata({
				id: 2,
				mime_type: "audio/mpeg",
				name: "Song Only.flac",
				size: 1,
			}),
		).toEqual({
			artist: null,
			artists: null,
			title: "Song Only",
		});
	});

	it("normalizes backend audio metadata before storing it on tracks", () => {
		expect(
			backendAudioMetadataToTrackMetadata({
				kind: "audio",
				metadata: {
					album: " Album ",
					artist: "Primary Artist",
					artists: [" ", "Featured Artist"],
					has_embedded_picture: false,
					kind: "audio",
					title: " Song ",
				},
				status: "ready",
			} as never),
		).toEqual({
			album: "Album",
			artist: "Primary Artist",
			artists: ["Featured Artist"],
			title: "Song",
		});
		expect(
			backendAudioMetadataToTrackMetadata({
				kind: "audio",
				metadata: {
					album: "Album Only",
					artist: " ",
					artists: null,
					has_embedded_picture: false,
					kind: "audio",
					title: "",
				},
				status: "ready",
			} as never),
		).toEqual({
			album: "Album Only",
		});
		expect(
			backendAudioMetadataToTrackMetadata({
				kind: "video",
				metadata: { kind: "video" },
				status: "ready",
			} as never),
		).toBeNull();
		expect(
			backendAudioMetadataToTrackMetadata({
				kind: "audio",
				metadata: {
					album: " ",
					artist: " ",
					artists: [],
					has_embedded_picture: false,
					kind: "audio",
					title: " ",
				},
				status: "ready",
			} as never),
		).toBeNull();
	});

	it("builds direct queues from only music files", async () => {
		const queue = await buildDirectMusicQueue([
			{
				file_category: "audio",
				id: 1,
				mime_type: "audio/mpeg",
				name: "Artist - Song.mp3",
				size: 10,
			},
			{
				file_category: "document",
				id: 2,
				mime_type: "application/pdf",
				name: "Manual.pdf",
				size: 20,
			},
		]);

		expect(queue).toEqual([
			expect.objectContaining({
				id: "file:1",
				metadata: {
					artist: "Artist",
					artists: ["Artist"],
					title: "Song",
				},
				path: "/files/1/download?disposition=inline",
				resource: expect.objectContaining({
					request: expect.objectContaining({
						url: "/files/1/download?disposition=inline",
					}),
				}),
				thumbnail: {
					file: {
						file_category: "audio",
						id: 1,
						mime_type: "audio/mpeg",
						name: "Artist - Song.mp3",
					},
					path: "/files/1/thumbnail",
				},
			}),
		]);
	});

	it("skips backend metadata at call time when media data support rejects the file", async () => {
		mockState.mediaDataSupportStore.config = {
			enabled: true,
			kinds: {
				audio: { enabled: true, extensions: ["flac"], match: "extensions" },
				image: { enabled: true, extensions: ["jpg"], match: "extensions" },
				video: { enabled: false, extensions: [], match: "extensions" },
			},
			max_source_bytes: 100,
			version: 1,
		};

		const [unsupportedExtensionTrack] = await buildDirectMusicQueue([
			{
				file_category: "audio",
				id: 11,
				mime_type: "audio/mpeg",
				name: "Direct.mp3",
				size: 10,
			},
		]);
		const [oversizedTrack] = await buildDirectMusicQueue([
			{
				file_category: "audio",
				id: 12,
				mime_type: "audio/flac",
				name: "Large.flac",
				size: 101,
			},
		]);

		await expect(
			unsupportedExtensionTrack?.loadBackendMetadata?.(),
		).resolves.toBeNull();
		await expect(oversizedTrack?.loadBackendMetadata?.()).resolves.toBeNull();
		expect(mockState.getFileMediaMetadata).not.toHaveBeenCalled();
	});

	it("checks backend metadata support when the loader is invoked", async () => {
		mockState.mediaDataSupportStore.config = null;
		mockState.getFileMediaMetadata.mockResolvedValueOnce({
			kind: "audio",
			metadata: {
				artist: "Late Artist",
				has_embedded_picture: false,
				kind: "audio",
				title: "Late Song",
			},
			status: "ready",
		});

		const [track] = await buildDirectMusicQueue([
			{
				file_category: "audio",
				id: 13,
				mime_type: "audio/mpeg",
				name: "Late.mp3",
				size: 10,
			},
		]);

		mockState.mediaDataSupportStore.config = {
			enabled: true,
			kinds: {
				audio: { enabled: true, extensions: ["mp3"], match: "extensions" },
				image: { enabled: true, extensions: ["jpg"], match: "extensions" },
				video: { enabled: false, extensions: [], match: "extensions" },
			},
			max_source_bytes: 1024,
			version: 1,
		};

		await expect(track?.loadBackendMetadata?.()).resolves.toEqual({
			artist: "Late Artist",
			artists: ["Late Artist"],
			title: "Late Song",
		});
		expect(mockState.getFileMediaMetadata).toHaveBeenCalledWith(13, {
			signal: undefined,
		});
	});

	it("loads backend metadata for direct and share tracks through the right service routes", async () => {
		const signal = new AbortController().signal;
		mockState.getFileMediaMetadata.mockResolvedValueOnce({
			kind: "audio",
			metadata: {
				artist: "Direct Artist",
				has_embedded_picture: false,
				kind: "audio",
				title: "Direct Song",
			},
			status: "ready",
		});
		mockState.getShareMediaMetadata.mockResolvedValueOnce({
			kind: "audio",
			metadata: {
				artist: "Share Artist",
				has_embedded_picture: false,
				kind: "audio",
				title: "Share Song",
			},
			status: "ready",
		});
		mockState.getShareFolderFileMediaMetadata.mockResolvedValueOnce({
			kind: "audio",
			metadata: {
				artist: "Folder Artist",
				has_embedded_picture: false,
				kind: "audio",
				title: "Folder Song",
			},
			status: "ready",
		});

		const [directTrack] = await buildDirectMusicQueue([
			{
				file_category: "audio",
				id: 11,
				mime_type: "audio/mpeg",
				name: "Direct.mp3",
			},
		]);
		const singleShareTrack = buildSingleShareMusicTrack(
			{
				download_count: 0,
				has_password: false,
				mime_type: "audio/mpeg",
				name: "Share.mp3",
				shared_by: { avatar: null, name: "Alice" },
				share_type: "file",
				size: 128,
			},
			"share-token",
		);
		const [folderTrack] = buildShareFolderMusicQueue("share-token", [
			{
				file_category: "audio",
				id: 12,
				mime_type: "audio/mpeg",
				name: "Folder.mp3",
			},
		]);

		await expect(directTrack?.loadBackendMetadata?.(signal)).resolves.toEqual({
			artist: "Direct Artist",
			artists: ["Direct Artist"],
			title: "Direct Song",
		});
		await expect(
			singleShareTrack?.loadBackendMetadata?.(signal),
		).resolves.toEqual({
			artist: "Share Artist",
			artists: ["Share Artist"],
			title: "Share Song",
		});
		await expect(folderTrack?.loadBackendMetadata?.(signal)).resolves.toEqual({
			artist: "Folder Artist",
			artists: ["Folder Artist"],
			title: "Folder Song",
		});
		expect(mockState.getFileMediaMetadata).toHaveBeenCalledWith(11, { signal });
		expect(mockState.getShareMediaMetadata).toHaveBeenCalledWith(
			"share-token",
			{
				signal,
			},
		);
		expect(mockState.getShareFolderFileMediaMetadata).toHaveBeenCalledWith(
			"share-token",
			12,
			{ signal },
		);
	});

	it("builds share queues with refreshable stream sessions", async () => {
		mockState.createFolderFileStreamSession.mockResolvedValueOnce({
			expires_at: "2026-01-01T00:00:00Z",
			path: "/api/v1/s/share-token/stream/session/1.mp3",
		});
		const queue = buildShareFolderMusicQueue("share-token", [
			{
				file_category: "audio",
				id: 1,
				mime_type: "audio/mpeg",
				name: "Song.mp3",
				size: 10,
			},
		]);

		expect(queue[0]).toMatchObject({
			id: "share:share-token:file:1",
			path: "/s/share-token/files/1/download",
			thumbnail: {
				file: {
					file_category: "audio",
					id: 1,
					mime_type: "audio/mpeg",
					name: "Song.mp3",
				},
				path: "/s/share-token/files/1/thumbnail",
			},
		});

		const hydrated = await hydrateMusicQueueForPlayback(
			queue,
			"share:share-token:file:1",
		);

		expect(mockState.createFolderFileStreamSession).toHaveBeenCalledWith(
			"share-token",
			1,
		);
		expect(hydrated[0]).toMatchObject({
			expiresAt: "2026-01-01T00:00:00Z",
			path: "/api/v1/s/share-token/stream/session/1.mp3",
		});
	});

	it("builds a single share track only for shared music files", async () => {
		mockState.createStreamSession.mockResolvedValueOnce({
			expires_at: "2026-01-01T00:00:00Z",
			path: "/api/v1/s/share-token/stream/session/file.mp3",
		});
		const track = buildSingleShareMusicTrack(
			{
				download_count: 0,
				has_password: false,
				mime_type: "audio/mpeg",
				name: "Shared.mp3",
				shared_by: { avatar: null, name: "Alice" },
				share_type: "file",
				size: 128,
			},
			"share-token",
		);

		expect(track).toMatchObject({
			id: "share:share-token:file",
			path: "/s/share-token/download",
			thumbnail: {
				file: {
					file_category: "audio",
					id: -1,
					mime_type: "audio/mpeg",
					name: "Shared.mp3",
				},
				path: "/s/share-token/thumbnail",
			},
		});

		const hydrated = await hydrateMusicQueueForPlayback(
			track ? [track] : [],
			"share:share-token:file",
		);

		expect(mockState.createStreamSession).toHaveBeenCalledWith("share-token");
		expect(hydrated[0]).toMatchObject({
			expiresAt: "2026-01-01T00:00:00Z",
			path: "/api/v1/s/share-token/stream/session/file.mp3",
		});
	});

	it("returns null for single share tracks without usable audio metadata", () => {
		expect(
			buildSingleShareMusicTrack(
				{
					download_count: 0,
					has_password: false,
					mime_type: null,
					name: "Shared.mp3",
					shared_by: { avatar: null, name: "Alice" },
					share_type: "file",
					size: 128,
				},
				"share-token",
			),
		).toBeNull();
		expect(
			buildSingleShareMusicTrack(
				{
					download_count: 0,
					has_password: false,
					mime_type: "application/pdf",
					name: "Manual.pdf",
					shared_by: { avatar: null, name: "Alice" },
					share_type: "file",
					size: 128,
				},
				"share-token",
			),
		).toBeNull();
	});

	it("does not hydrate when the active queue track is missing or has no refresh hook", async () => {
		const directTrack = {
			id: "file:1",
			mimeType: "audio/mpeg",
			name: "Song.mp3",
			path: "/files/1/download",
			resource: "/files/1/download",
		};

		await expect(hydrateMusicTrackStreamLink(directTrack)).resolves.toBe(
			directTrack,
		);
		await expect(
			hydrateMusicQueueForPlayback([directTrack], "missing-track"),
		).resolves.toEqual([directTrack]);
	});

	it("drops empty stream-session expirations while hydrating a track", async () => {
		const track = {
			id: "share:token:file",
			mimeType: "audio/mpeg",
			name: "Song.mp3",
			path: "/old",
			resource: "/old",
			refreshStreamLink: vi.fn(async () => ({
				expires_at: "",
				path: "/new",
			})),
		};

		await expect(hydrateMusicTrackStreamLink(track)).resolves.toEqual({
			...track,
			expiresAt: undefined,
			path: "/new",
			resource: expect.objectContaining({
				request: expect.objectContaining({
					url: "/new",
				}),
			}),
		});
	});
});
