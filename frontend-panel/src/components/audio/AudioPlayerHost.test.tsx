import { act, fireEvent, render, screen } from "@testing-library/react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { AudioPlayerHost } from "@/components/audio/AudioPlayerHost";

const mockState = vi.hoisted(() => ({
	clear: vi.fn(),
	requestPlayback: vi.fn(),
	setError: vi.fn(),
	setPlaybackRequested: vi.fn(),
	setPlaying: vi.fn(),
	updateTrackSource: vi.fn(),
	state: {
		error: null as string | null,
		isPlaying: false,
		playRequestVersion: 0,
		playRequested: false,
		track: null as {
			expiresAt?: string;
			refreshStreamLink?: () => Promise<{
				expires_at: string;
				path: string;
			}>;
			id: string;
			mimeType: string;
			name: string;
			path: string;
			size?: number;
		} | null,
	},
}));

vi.mock("react-i18next", () => ({
	useTranslation: () => ({
		t: (key: string) => key,
	}),
}));

vi.mock("@/stores/audioPlayerStore", () => ({
	useAudioPlayerStore: (
		selector: (state: {
			clear: typeof mockState.clear;
			error: string | null;
			isPlaying: boolean;
			playRequestVersion: number;
			playRequested: boolean;
			requestPlayback: typeof mockState.requestPlayback;
			setError: typeof mockState.setError;
			setPlaybackRequested: typeof mockState.setPlaybackRequested;
			setPlaying: typeof mockState.setPlaying;
			track: typeof mockState.state.track;
			updateTrackSource: typeof mockState.updateTrackSource;
		}) => unknown,
	) =>
		selector({
			...mockState.state,
			clear: mockState.clear,
			requestPlayback: mockState.requestPlayback,
			setError: mockState.setError,
			setPlaybackRequested: mockState.setPlaybackRequested,
			setPlaying: mockState.setPlaying,
			updateTrackSource: mockState.updateTrackSource,
		}),
}));

describe("AudioPlayerHost", () => {
	beforeEach(() => {
		vi.useRealTimers();
		Object.defineProperty(HTMLMediaElement.prototype, "play", {
			configurable: true,
			value: vi.fn(() => Promise.resolve()),
		});
		Object.defineProperty(HTMLMediaElement.prototype, "pause", {
			configurable: true,
			value: vi.fn(),
		});
		mockState.clear.mockReset();
		mockState.requestPlayback.mockReset();
		mockState.setError.mockReset();
		mockState.setPlaybackRequested.mockReset();
		mockState.setPlaying.mockReset();
		mockState.updateTrackSource.mockReset();
		mockState.state.error = null;
		mockState.state.isPlaying = false;
		mockState.state.playRequestVersion = 0;
		mockState.state.playRequested = false;
		mockState.state.track = null;
	});

	afterEach(() => {
		vi.useRealTimers();
	});

	it("renders nothing when no track is loaded", () => {
		const { container } = render(<AudioPlayerHost />);

		expect(container).toBeEmptyDOMElement();
	});

	it("renders the loaded track and can close the player", () => {
		mockState.state.track = {
			id: "track-1",
			mimeType: "audio/mpeg",
			name: "track.mp3",
			path: "/files/7/download",
			size: 1024,
		};

		render(<AudioPlayerHost />);

		expect(screen.getByText("track.mp3")).toBeInTheDocument();
		expect(document.querySelector("audio")).toHaveAttribute(
			"src",
			"/api/v1/files/7/download",
		);

		fireEvent.click(screen.getByRole("button", { name: "audio_player_close" }));

		expect(mockState.clear).toHaveBeenCalledTimes(1);
	});

	it("requests playback when play is clicked", () => {
		mockState.state.track = {
			id: "track-1",
			mimeType: "audio/mpeg",
			name: "track.mp3",
			path: "/files/7/download",
		};

		render(<AudioPlayerHost />);

		fireEvent.click(screen.getByRole("button", { name: "audio_player_play" }));

		expect(mockState.requestPlayback).toHaveBeenCalledTimes(1);
	});

	it("pauses playback when pause is clicked", () => {
		mockState.state.isPlaying = true;
		mockState.state.playRequested = true;
		mockState.state.track = {
			id: "track-1",
			mimeType: "audio/mpeg",
			name: "track.mp3",
			path: "/files/7/download",
		};

		render(<AudioPlayerHost />);

		fireEvent.click(screen.getByRole("button", { name: "audio_player_pause" }));

		expect(mockState.setPlaybackRequested).toHaveBeenCalledWith(false);
	});

	it("reflects audio element events back into the player store", () => {
		mockState.state.track = {
			id: "track-1",
			mimeType: "audio/mpeg",
			name: "track.mp3",
			path: "/files/7/download",
		};

		render(<AudioPlayerHost />);

		const audio = document.querySelector("audio");
		if (!audio) {
			throw new Error("audio element not found");
		}

		fireEvent.play(audio);
		expect(mockState.setError).toHaveBeenCalledWith(null);
		expect(mockState.setPlaying).toHaveBeenCalledWith(true);
		expect(mockState.setPlaybackRequested).toHaveBeenCalledWith(true);

		fireEvent.pause(audio);
		expect(mockState.setPlaying).toHaveBeenCalledWith(false);

		fireEvent.error(audio);
		expect(mockState.setError).toHaveBeenCalledWith("audio_player_load_failed");
		expect(mockState.setPlaybackRequested).toHaveBeenCalledWith(false);
		expect(mockState.setPlaying).toHaveBeenCalledWith(false);
	});

	it("updates the seek control from audio metadata and lets users seek", () => {
		mockState.state.track = {
			id: "track-1",
			mimeType: "audio/mpeg",
			name: "track.mp3",
			path: "/files/7/download",
		};

		render(<AudioPlayerHost />);

		const audio = document.querySelector("audio");
		if (!audio) {
			throw new Error("audio element not found");
		}
		Object.defineProperty(audio, "duration", {
			configurable: true,
			value: 120,
		});
		Object.defineProperty(audio, "currentTime", {
			configurable: true,
			writable: true,
			value: 30,
		});

		fireEvent.loadedMetadata(audio);
		fireEvent.timeUpdate(audio);

		const seek = screen.getByRole("slider", { name: "audio_player_seek" });
		expect(seek).toHaveValue("25");

		fireEvent.change(seek, { target: { value: "50" } });

		expect(audio.currentTime).toBe(60);
		expect(seek).toHaveValue("50");
	});

	it("refreshes expiring stream sessions before the current link expires", async () => {
		vi.useFakeTimers();
		const refreshStreamLink = vi.fn(async () => ({
			expires_at: "2026-01-01T03:00:00Z",
			path: "/api/v1/s/share-token/stream/session-2/track.mp3",
		}));
		vi.setSystemTime(new Date("2026-01-01T00:00:00Z"));
		mockState.state.track = {
			expiresAt: "2026-01-01T00:03:00Z",
			id: "track-1",
			mimeType: "audio/mpeg",
			name: "track.mp3",
			path: "/api/v1/s/share-token/stream/session-1/track.mp3",
			refreshStreamLink,
		};

		render(<AudioPlayerHost />);

		await act(async () => {
			vi.advanceTimersByTime(60_000);
			await Promise.resolve();
		});

		expect(refreshStreamLink).toHaveBeenCalledTimes(1);
		expect(mockState.updateTrackSource).toHaveBeenCalledWith("track-1", {
			expires_at: "2026-01-01T03:00:00Z",
			path: "/api/v1/s/share-token/stream/session-2/track.mp3",
		});
	});
});
