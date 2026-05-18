import { beforeEach, describe, expect, it, vi } from "vitest";

async function loadMusicPlayerStore() {
	vi.resetModules();
	return await import("@/stores/musicPlayerStore");
}

describe("musicPlayerStore", () => {
	beforeEach(() => {
		vi.useRealTimers();
	});

	it("starts playback for a new track and records a play request version", async () => {
		const { useMusicPlayerStore } = await loadMusicPlayerStore();

		useMusicPlayerStore.getState().playTrack({
			id: "track-1",
			mimeType: "audio/mpeg",
			name: "track.mp3",
			path: "/files/7/download",
			size: 4096,
		});

		expect(useMusicPlayerStore.getState()).toMatchObject({
			activeTrackId: "track-1",
			error: null,
			playRequested: true,
			playRequestVersion: 1,
			queue: [
				{
					id: "track-1",
					mimeType: "audio/mpeg",
					name: "track.mp3",
					path: "/files/7/download",
					size: 4096,
				},
			],
		});
	});

	it("loads a queue and starts the requested active track", async () => {
		const { useMusicPlayerStore } = await loadMusicPlayerStore();

		useMusicPlayerStore.getState().playTracks(
			[
				{
					id: "track-1",
					mimeType: "audio/mpeg",
					name: "one.mp3",
					path: "/files/1/download",
				},
				{
					id: "track-2",
					mimeType: "audio/mpeg",
					name: "two.mp3",
					path: "/files/2/download",
				},
			],
			"track-2",
		);

		expect(useMusicPlayerStore.getState()).toMatchObject({
			activeTrackId: "track-2",
			playRequested: true,
			queue: [
				expect.objectContaining({ id: "track-1" }),
				expect.objectContaining({ id: "track-2" }),
			],
		});
	});

	it("deduplicates queued tracks by id", async () => {
		const { useMusicPlayerStore } = await loadMusicPlayerStore();

		useMusicPlayerStore.getState().playTracks(
			[
				{
					id: "track-1",
					mimeType: "audio/mpeg",
					name: "one.mp3",
					path: "/files/1/download",
				},
				{
					id: "track-1",
					mimeType: "audio/mpeg",
					name: "duplicate.mp3",
					path: "/files/1-duplicate/download",
				},
			],
			"track-1",
		);

		expect(useMusicPlayerStore.getState().queue).toHaveLength(1);
		expect(useMusicPlayerStore.getState().queue[0]).toMatchObject({
			name: "one.mp3",
		});
	});

	it("increments the play request version when playback is requested again", async () => {
		const { useMusicPlayerStore } = await loadMusicPlayerStore();

		useMusicPlayerStore.getState().playTrack({
			id: "track-1",
			mimeType: "audio/mpeg",
			name: "track.mp3",
			path: "/files/7/download",
		});
		useMusicPlayerStore.getState().setPlaybackRequested(false);
		useMusicPlayerStore.getState().requestPlayback();

		expect(useMusicPlayerStore.getState()).toMatchObject({
			playRequested: true,
			playRequestVersion: 2,
		});
	});

	it("moves through the queue and wraps in repeat queue mode", async () => {
		const { useMusicPlayerStore } = await loadMusicPlayerStore();

		useMusicPlayerStore.getState().playTracks(
			[
				{
					id: "track-1",
					mimeType: "audio/mpeg",
					name: "one.mp3",
					path: "/files/1/download",
				},
				{
					id: "track-2",
					mimeType: "audio/mpeg",
					name: "two.mp3",
					path: "/files/2/download",
				},
			],
			"track-2",
		);
		useMusicPlayerStore.getState().playNext();

		expect(useMusicPlayerStore.getState().activeTrackId).toBe("track-1");

		useMusicPlayerStore.getState().playPrevious();

		expect(useMusicPlayerStore.getState().activeTrackId).toBe("track-2");
	});

	it("falls back to the first track when the active track is missing", async () => {
		const { useMusicPlayerStore } = await loadMusicPlayerStore();

		useMusicPlayerStore.getState().playTracks(
			[
				{
					id: "track-1",
					mimeType: "audio/mpeg",
					name: "one.mp3",
					path: "/files/1/download",
				},
				{
					id: "track-2",
					mimeType: "audio/mpeg",
					name: "two.mp3",
					path: "/files/2/download",
				},
			],
			"track-1",
		);
		useMusicPlayerStore.setState({ activeTrackId: "missing-track" });

		useMusicPlayerStore.getState().playNext();
		expect(useMusicPlayerStore.getState().activeTrackId).toBe("track-1");

		useMusicPlayerStore.setState({ activeTrackId: "missing-track" });
		useMusicPlayerStore.getState().playPrevious();
		expect(useMusicPlayerStore.getState().activeTrackId).toBe("track-1");
	});

	it("uses a different queued track for shuffle next and previous", async () => {
		const { useMusicPlayerStore } = await loadMusicPlayerStore();
		const randomSpy = vi.spyOn(Math, "random").mockReturnValue(0);

		useMusicPlayerStore.getState().playTracks(
			[
				{
					id: "track-1",
					mimeType: "audio/mpeg",
					name: "one.mp3",
					path: "/files/1/download",
				},
				{
					id: "track-2",
					mimeType: "audio/mpeg",
					name: "two.mp3",
					path: "/files/2/download",
				},
				{
					id: "track-3",
					mimeType: "audio/mpeg",
					name: "three.mp3",
					path: "/files/3/download",
				},
			],
			"track-1",
		);
		useMusicPlayerStore.getState().setPlaybackMode("shuffle");

		useMusicPlayerStore.getState().playNext();
		expect(useMusicPlayerStore.getState().activeTrackId).toBe("track-2");

		useMusicPlayerStore.setState({ activeTrackId: "track-1" });
		useMusicPlayerStore.getState().playPrevious();
		expect(useMusicPlayerStore.getState().activeTrackId).toBe("track-2");

		randomSpy.mockRestore();
	});

	it("stops playback when next is requested with an empty queue", async () => {
		const { useMusicPlayerStore } = await loadMusicPlayerStore();

		useMusicPlayerStore.setState({
			activeTrackId: "missing-track",
			isPlaying: true,
			playRequested: true,
			queue: [],
		});

		useMusicPlayerStore.getState().playNext();

		expect(useMusicPlayerStore.getState()).toMatchObject({
			activeTrackId: null,
			isPlaying: false,
			playRequested: false,
		});
	});

	it("leaves state unchanged when previous or a missing queue track is requested without a match", async () => {
		const { useMusicPlayerStore } = await loadMusicPlayerStore();

		useMusicPlayerStore.getState().playTrack({
			id: "track-1",
			mimeType: "audio/mpeg",
			name: "one.mp3",
			path: "/files/1/download",
		});
		const beforeMissingQueueTrack = useMusicPlayerStore.getState();

		useMusicPlayerStore.getState().playTracks(
			[
				{
					id: "track-2",
					mimeType: "audio/mpeg",
					name: "two.mp3",
					path: "/files/2/download",
				},
			],
			"track-3",
		);
		expect(useMusicPlayerStore.getState()).toBe(beforeMissingQueueTrack);

		useMusicPlayerStore.setState({ queue: [] });
		const beforePrevious = useMusicPlayerStore.getState();

		useMusicPlayerStore.getState().playPrevious();

		expect(useMusicPlayerStore.getState()).toBe(beforePrevious);
	});

	it("still moves to adjacent tracks when repeat one is enabled", async () => {
		const { useMusicPlayerStore } = await loadMusicPlayerStore();

		useMusicPlayerStore.getState().playTracks(
			[
				{
					id: "track-1",
					mimeType: "audio/mpeg",
					name: "one.mp3",
					path: "/files/1/download",
				},
				{
					id: "track-2",
					mimeType: "audio/mpeg",
					name: "two.mp3",
					path: "/files/2/download",
				},
			],
			"track-1",
		);
		useMusicPlayerStore.getState().setPlaybackMode("repeat_one");
		useMusicPlayerStore.getState().playNext();

		expect(useMusicPlayerStore.getState().activeTrackId).toBe("track-2");

		useMusicPlayerStore.getState().playPrevious();

		expect(useMusicPlayerStore.getState().activeTrackId).toBe("track-1");
	});

	it("falls back to queued tracks when repeat one points at a missing track", async () => {
		const { useMusicPlayerStore } = await loadMusicPlayerStore();

		useMusicPlayerStore.setState({
			activeTrackId: "missing-track",
			playbackMode: "repeat_one",
			queue: [
				{
					id: "track-1",
					mimeType: "audio/mpeg",
					name: "one.mp3",
					path: "/files/1/download",
				},
			],
		});

		useMusicPlayerStore.getState().playNext();
		expect(useMusicPlayerStore.getState().activeTrackId).toBe("track-1");

		useMusicPlayerStore.setState({ activeTrackId: "missing-track" });
		useMusicPlayerStore.getState().playPrevious();
		expect(useMusicPlayerStore.getState().activeTrackId).toBe("track-1");
	});

	it("updates an existing queued track when playTrack receives the same id", async () => {
		const { useMusicPlayerStore } = await loadMusicPlayerStore();

		useMusicPlayerStore.getState().playTrack({
			id: "track-1",
			mimeType: "audio/mpeg",
			name: "old.mp3",
			path: "/files/1/download",
		});
		useMusicPlayerStore.getState().playTrack({
			id: "track-1",
			mimeType: "audio/flac",
			name: "updated.flac",
			path: "/files/1/new-download",
		});

		expect(useMusicPlayerStore.getState().queue).toEqual([
			expect.objectContaining({
				id: "track-1",
				mimeType: "audio/flac",
				name: "updated.flac",
				path: "/files/1/new-download",
			}),
		]);
	});

	it("updates only the matching queued track source after a stream session refresh", async () => {
		const { useMusicPlayerStore } = await loadMusicPlayerStore();

		useMusicPlayerStore.getState().playTracks(
			[
				{
					expiresAt: "2026-01-01T00:30:00Z",
					id: "track-1",
					mimeType: "audio/mpeg",
					name: "one.mp3",
					path: "/api/v1/s/share-token/stream/session-1/one.mp3",
				},
				{
					expiresAt: "2026-01-01T00:30:00Z",
					id: "track-2",
					mimeType: "audio/mpeg",
					name: "two.mp3",
					path: "/api/v1/s/share-token/stream/session-1/two.mp3",
				},
			],
			"track-1",
		);

		useMusicPlayerStore.getState().updateTrackSource("track-2", {
			expires_at: "2026-01-01T01:00:00Z",
			path: "/api/v1/s/share-token/stream/session-2/two.mp3",
		});

		expect(useMusicPlayerStore.getState().queue).toEqual([
			expect.objectContaining({
				id: "track-1",
				expiresAt: "2026-01-01T00:30:00Z",
				path: "/api/v1/s/share-token/stream/session-1/one.mp3",
			}),
			expect.objectContaining({
				id: "track-2",
				expiresAt: "2026-01-01T01:00:00Z",
				path: "/api/v1/s/share-token/stream/session-2/two.mp3",
			}),
		]);
	});

	it("ignores source updates for tracks that are no longer queued", async () => {
		const { useMusicPlayerStore } = await loadMusicPlayerStore();

		useMusicPlayerStore.getState().playTrack({
			id: "track-1",
			mimeType: "audio/mpeg",
			name: "one.mp3",
			path: "/files/1/download",
		});
		const before = useMusicPlayerStore.getState();

		useMusicPlayerStore.getState().updateTrackSource("missing-track", {
			expires_at: "2026-01-01T01:00:00Z",
			path: "/files/missing/download",
		});

		expect(useMusicPlayerStore.getState()).toBe(before);
	});

	it("toggles and directly sets the panel open state", async () => {
		const { useMusicPlayerStore } = await loadMusicPlayerStore();

		useMusicPlayerStore.getState().openPanel();
		expect(useMusicPlayerStore.getState().isPanelOpen).toBe(true);

		useMusicPlayerStore.getState().closePanel();
		expect(useMusicPlayerStore.getState().isPanelOpen).toBe(false);

		useMusicPlayerStore.getState().setPanelOpen(true);
		expect(useMusicPlayerStore.getState().isPanelOpen).toBe(true);

		useMusicPlayerStore.getState().togglePanel();
		expect(useMusicPlayerStore.getState().isPanelOpen).toBe(false);
	});

	it("merges parsed metadata into only the matching queued track", async () => {
		const { useMusicPlayerStore } = await loadMusicPlayerStore();

		useMusicPlayerStore.getState().playTracks(
			[
				{
					id: "track-1",
					metadata: { artist: "Artist One", title: "Track One" },
					mimeType: "audio/mpeg",
					name: "one.mp3",
					path: "/files/1/download",
				},
				{
					id: "track-2",
					metadata: { title: "Track Two" },
					mimeType: "audio/mpeg",
					name: "two.mp3",
					path: "/files/2/download",
				},
			],
			"track-1",
		);

		useMusicPlayerStore.getState().updateTrackMetadata("track-1", {
			album: "Album One",
			artworkUrl: "data:image/jpeg;base64,cover",
			title: "Parsed Title",
		});
		useMusicPlayerStore.getState().updateTrackMetadata("missing-track", {
			title: "Ignored",
		});

		expect(useMusicPlayerStore.getState().queue).toEqual([
			expect.objectContaining({
				id: "track-1",
				metadata: {
					album: "Album One",
					artist: "Artist One",
					artworkUrl: "data:image/jpeg;base64,cover",
					title: "Parsed Title",
				},
			}),
			expect.objectContaining({
				id: "track-2",
				metadata: { title: "Track Two" },
			}),
		]);
	});

	it("clears queue, playback state, errors, panel state, and request counters", async () => {
		const { useMusicPlayerStore } = await loadMusicPlayerStore();

		useMusicPlayerStore.getState().playTrack({
			id: "track-1",
			mimeType: "audio/mpeg",
			name: "track.mp3",
			path: "/files/7/download",
		});
		useMusicPlayerStore.getState().openPanel();
		useMusicPlayerStore.getState().setError("load failed");
		useMusicPlayerStore.getState().setPlaying(true);
		useMusicPlayerStore.getState().clear();

		expect(useMusicPlayerStore.getState()).toMatchObject({
			activeTrackId: null,
			error: null,
			isPanelOpen: false,
			isPlaying: false,
			playRequested: false,
			playRequestVersion: 0,
			queue: [],
		});
	});
});
