import { beforeEach, describe, expect, it, vi } from "vitest";

async function loadAudioPlayerStore() {
	vi.resetModules();
	return await import("@/stores/audioPlayerStore");
}

describe("audioPlayerStore", () => {
	beforeEach(() => {
		vi.useRealTimers();
	});

	it("starts playback for a new track and records a play request version", async () => {
		const { useAudioPlayerStore } = await loadAudioPlayerStore();

		useAudioPlayerStore.getState().playTrack({
			id: "track-1",
			mimeType: "audio/mpeg",
			name: "track.mp3",
			path: "/files/7/download",
			size: 4096,
		});

		expect(useAudioPlayerStore.getState()).toMatchObject({
			error: null,
			playRequested: true,
			playRequestVersion: 1,
			track: {
				id: "track-1",
				mimeType: "audio/mpeg",
				name: "track.mp3",
				path: "/files/7/download",
				size: 4096,
			},
		});
	});

	it("increments the play request version when playback is requested again", async () => {
		const { useAudioPlayerStore } = await loadAudioPlayerStore();

		useAudioPlayerStore.getState().playTrack({
			id: "track-1",
			mimeType: "audio/mpeg",
			name: "track.mp3",
			path: "/files/7/download",
		});
		useAudioPlayerStore.getState().setPlaybackRequested(false);
		useAudioPlayerStore.getState().requestPlayback();

		expect(useAudioPlayerStore.getState()).toMatchObject({
			playRequested: true,
			playRequestVersion: 2,
		});
	});

	it("updates only the active track source after a stream session refresh", async () => {
		const { useAudioPlayerStore } = await loadAudioPlayerStore();

		useAudioPlayerStore.getState().playTrack({
			expiresAt: "2026-01-01T00:30:00Z",
			id: "track-1",
			mimeType: "audio/mpeg",
			name: "track.mp3",
			path: "/api/v1/s/share-token/stream/session-1/track.mp3",
		});
		useAudioPlayerStore.getState().updateTrackSource("other-track", {
			expires_at: "2026-01-01T01:00:00Z",
			path: "/api/v1/s/share-token/stream/wrong/track.mp3",
		});

		expect(useAudioPlayerStore.getState().track).toMatchObject({
			expiresAt: "2026-01-01T00:30:00Z",
			path: "/api/v1/s/share-token/stream/session-1/track.mp3",
		});

		useAudioPlayerStore.getState().updateTrackSource("track-1", {
			expires_at: "2026-01-01T01:00:00Z",
			path: "/api/v1/s/share-token/stream/session-2/track.mp3",
		});

		expect(useAudioPlayerStore.getState().track).toMatchObject({
			expiresAt: "2026-01-01T01:00:00Z",
			path: "/api/v1/s/share-token/stream/session-2/track.mp3",
		});
	});

	it("clears track, playback state, errors, and request counters", async () => {
		const { useAudioPlayerStore } = await loadAudioPlayerStore();

		useAudioPlayerStore.getState().playTrack({
			id: "track-1",
			mimeType: "audio/mpeg",
			name: "track.mp3",
			path: "/files/7/download",
		});
		useAudioPlayerStore.getState().setError("load failed");
		useAudioPlayerStore.getState().setPlaying(true);
		useAudioPlayerStore.getState().clear();

		expect(useAudioPlayerStore.getState()).toMatchObject({
			error: null,
			isPlaying: false,
			playRequested: false,
			playRequestVersion: 0,
			track: null,
		});
	});
});
