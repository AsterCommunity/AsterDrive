import { beforeEach, describe, expect, it, vi } from "vitest";
import {
	buildDirectMusicQueue,
	buildShareFolderMusicQueue,
	buildSingleShareMusicTrack,
	hydrateMusicQueueForPlayback,
	hydrateMusicTrackStreamLink,
	inferMusicMetadata,
	isMusicFile,
	parseMusicMetadataFromSource,
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
	fetch: vi.fn(),
	parseBlob: vi.fn(),
	selectCover: vi.fn(),
}));

vi.mock("@/services/fileService", () => ({
	fileService: {
		downloadPath: (id: number) => mockState.downloadPath(id),
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
	},
}));

vi.mock("music-metadata", () => ({
	parseBlob: (...args: unknown[]) => mockState.parseBlob(...args),
	selectCover: (...args: unknown[]) => mockState.selectCover(...args),
}));

describe("musicPlayer helpers", () => {
	beforeEach(() => {
		mockState.createFolderFileStreamSession.mockReset();
		mockState.createStreamSession.mockReset();
		mockState.downloadFolderPath.mockClear();
		mockState.downloadPath.mockClear();
		mockState.fetch.mockReset();
		mockState.parseBlob.mockReset();
		mockState.selectCover.mockReset();
		vi.stubGlobal("fetch", mockState.fetch);
		vi.stubGlobal("btoa", (value: string) =>
			Buffer.from(value, "binary").toString("base64"),
		);
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

	it("builds direct queues from only music files", () => {
		const queue = buildDirectMusicQueue([
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
				path: "/files/1/download",
			}),
		]);
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
		};

		await expect(hydrateMusicTrackStreamLink(directTrack)).resolves.toBe(
			directTrack,
		);
		await expect(
			hydrateMusicQueueForPlayback([directTrack], "missing-track"),
		).resolves.toEqual([directTrack]);
	});

	it("parses browser music metadata and turns embedded cover art into a data URL", async () => {
		const blob = new Blob(["audio"]);
		mockState.fetch.mockResolvedValueOnce({
			blob: async () => blob,
			headers: new Headers({ "Content-Range": "bytes 0-4/100" }),
			ok: true,
			status: 206,
		});
		mockState.parseBlob.mockResolvedValueOnce({
			common: {
				album: "Album One",
				artist: "First Artist",
				artists: ["First Artist", "Second Artist"],
				picture: [{ data: new Uint8Array([1, 2, 3]), format: "image/png" }],
				title: "Parsed Song",
			},
		});
		mockState.selectCover.mockReturnValueOnce({
			data: new Uint8Array([1, 2, 3]),
			format: "image/png",
		});

		const metadata = await parseMusicMetadataFromSource({
			fallbackMetadata: { artist: "Fallback Artist", title: "Fallback Song" },
			mimeType: "audio/mpeg",
			name: "Fallback Song.mp3",
			size: 123,
			source: "/api/v1/files/1/download",
		});

		expect(mockState.fetch).toHaveBeenCalledWith(
			"/api/v1/files/1/download",
			expect.objectContaining({
				credentials: "include",
				headers: expect.any(Headers),
			}),
		);
		expect(
			(mockState.fetch.mock.calls[0]?.[1] as { headers: Headers }).headers.get(
				"Range",
			),
		).toBe("bytes=0-3145727");
		expect(mockState.parseBlob).toHaveBeenCalledWith(
			blob,
			expect.objectContaining({
				duration: false,
				skipCovers: false,
				skipPostHeaders: true,
			}),
		);
		expect(metadata).toEqual({
			album: "Album One",
			artist: "First Artist, Second Artist",
			artists: ["First Artist", "Second Artist"],
			artworkUrl: "data:image/png;base64,AQID",
			title: "Parsed Song",
		});
	});

	it("keeps fallback metadata when parsed tags are missing cover, title, or artist", async () => {
		mockState.fetch.mockResolvedValueOnce({
			blob: async () => new Blob(["audio"]),
			headers: new Headers(),
			ok: true,
			status: 206,
		});
		mockState.parseBlob.mockResolvedValueOnce({
			common: {
				album: "",
				artist: "",
				artists: [],
				picture: [],
				title: "",
			},
		});
		mockState.selectCover.mockReturnValueOnce(null);

		await expect(
			parseMusicMetadataFromSource({
				fallbackMetadata: {
					artist: "Fallback Artist",
					artworkUrl: "data:image/jpeg;base64,old",
					title: "Fallback Title",
				},
				mimeType: "audio/flac",
				name: "File Name.flac",
				source: "blob:local-audio",
			}),
		).resolves.toEqual({
			album: undefined,
			artist: "Fallback Artist",
			artists: ["Fallback Artist"],
			artworkUrl: "data:image/jpeg;base64,old",
			title: "Fallback Title",
		});
		expect(
			(mockState.fetch.mock.calls[0]?.[1] as { headers: Headers }).headers.get(
				"Range",
			),
		).toBeNull();
	});

	it("skips parsing when a ranged metadata request returns an unbounded full body", async () => {
		const blob = vi.fn(async () => new Blob(["full body"]));
		mockState.fetch.mockResolvedValueOnce({
			blob,
			headers: new Headers(),
			ok: true,
			status: 200,
		});

		await expect(
			parseMusicMetadataFromSource({
				fallbackMetadata: { artist: "Fallback Artist", title: "Fallback Song" },
				mimeType: "audio/mpeg",
				name: "Fallback Song.mp3",
				size: 10_000_000,
				source: "/api/v1/files/1/download",
			}),
		).resolves.toEqual({
			artist: "Fallback Artist",
			title: "Fallback Song",
		});
		expect(blob).not.toHaveBeenCalled();
		expect(mockState.parseBlob).not.toHaveBeenCalled();
	});

	it("throws when metadata fetch fails", async () => {
		mockState.fetch.mockResolvedValueOnce({
			blob: async () => new Blob(["audio"]),
			headers: new Headers(),
			ok: false,
			status: 500,
		});

		await expect(
			parseMusicMetadataFromSource({
				mimeType: "audio/mpeg",
				name: "Broken.mp3",
				source: "/api/v1/files/1/download",
			}),
		).rejects.toThrow("music metadata request failed with 500");
		expect(mockState.parseBlob).not.toHaveBeenCalled();
	});
});
