import { fireEvent, render, screen, waitFor } from "@testing-library/react";
import { beforeEach, describe, expect, it, vi } from "vitest";
import { AudioPreview } from "@/components/files/preview/AudioPreview";

const mockState = vi.hoisted(() => ({
	audioStore: {
		isPlaying: false,
		requestPlayback: vi.fn(),
		track: null as {
			id: string;
			name: string;
			path: string;
		} | null,
	},
	playTrack: vi.fn(),
	warn: vi.fn(),
}));

vi.mock("react-i18next", () => ({
	useTranslation: () => ({
		t: (key: string) => key,
	}),
}));

vi.mock("@/lib/logger", () => ({
	logger: {
		warn: (...args: unknown[]) => mockState.warn(...args),
	},
}));

vi.mock("@/stores/audioPlayerStore", () => ({
	useAudioPlayerStore: (
		selector: (state: {
			isPlaying: boolean;
			playTrack: (...args: unknown[]) => void;
			requestPlayback: (...args: unknown[]) => void;
			track: typeof mockState.audioStore.track;
		}) => unknown,
	) =>
		selector({
			isPlaying: mockState.audioStore.isPlaying,
			playTrack: (...args: unknown[]) => mockState.playTrack(...args),
			requestPlayback: (...args: unknown[]) =>
				mockState.audioStore.requestPlayback(...args),
			track: mockState.audioStore.track,
		}),
}));

describe("AudioPreview", () => {
	beforeEach(() => {
		mockState.audioStore.isPlaying = false;
		mockState.audioStore.requestPlayback.mockReset();
		mockState.audioStore.track = null;
		mockState.playTrack.mockReset();
		mockState.warn.mockReset();
	});

	it("renders a playback entry point without creating a blob URL", () => {
		render(
			<AudioPreview
				file={{ name: "track.mp3", mime_type: "audio/mpeg" }}
				path="/files/7/download"
			/>,
		);

		expect(document.querySelector("audio")).toBeNull();
		expect(screen.getByText("track.mp3")).toBeInTheDocument();
		expect(screen.getByText("audio_preview_idle")).toBeInTheDocument();
		expect(
			screen.getByRole("button", { name: "audio_preview_play" }),
		).toBeInTheDocument();
	});

	it("starts direct audio playback through the global player store", async () => {
		render(
			<AudioPreview
				file={{ name: "track.mp3", mime_type: "audio/mpeg" }}
				path="/files/7/download"
			/>,
		);

		fireEvent.click(screen.getByRole("button", { name: "audio_preview_play" }));

		await waitFor(() => {
			expect(mockState.playTrack).toHaveBeenCalledWith(
				expect.objectContaining({
					name: "track.mp3",
					path: "/files/7/download",
				}),
			);
		});
	});

	it("uses a stable track id including file size to avoid resuming a different file", async () => {
		render(
			<AudioPreview
				file={{ name: "track.mp3", mime_type: "audio/mpeg", size: 4096 }}
				path="/files/7/download"
			/>,
		);

		fireEvent.click(screen.getByRole("button", { name: "audio_preview_play" }));

		await waitFor(() => {
			expect(mockState.playTrack).toHaveBeenCalledWith(
				expect.objectContaining({
					id: "track.mp3:4096:audio/mpeg:/files/7/download",
					size: 4096,
				}),
			);
		});
	});

	it("creates a stream session before starting share audio playback", async () => {
		const mediaStreamLinkFactory = vi.fn(async () => ({
			expires_at: "2026-01-01T00:00:00Z",
			path: "/api/v1/s/share-token/stream/session-token/track.mp3",
		}));

		render(
			<AudioPreview
				file={{ name: "track.mp3", mime_type: "audio/mpeg" }}
				path="/s/share-token/download"
				mediaStreamLinkFactory={mediaStreamLinkFactory}
			/>,
		);

		fireEvent.click(screen.getByRole("button", { name: "audio_preview_play" }));

		await waitFor(() => {
			expect(mockState.playTrack).toHaveBeenCalledWith(
				expect.objectContaining({
					expiresAt: "2026-01-01T00:00:00Z",
					path: "/api/v1/s/share-token/stream/session-token/track.mp3",
					refreshStreamLink: mediaStreamLinkFactory,
				}),
			);
		});
		expect(mediaStreamLinkFactory).toHaveBeenCalledTimes(1);
	});

	it("disables the action while a stream session is being created", () => {
		const mediaStreamLinkFactory = vi.fn(
			() =>
				new Promise<{
					expires_at: string;
					path: string;
				}>(() => {}),
		);

		render(
			<AudioPreview
				file={{ name: "track.mp3", mime_type: "audio/mpeg" }}
				path="/s/share-token/download"
				mediaStreamLinkFactory={mediaStreamLinkFactory}
			/>,
		);

		fireEvent.click(screen.getByRole("button", { name: "audio_preview_play" }));

		expect(
			screen.getByRole("button", { name: "loading_preview" }),
		).toBeDisabled();
		expect(mediaStreamLinkFactory).toHaveBeenCalledTimes(1);
	});

	it("renders the preview error when stream session creation fails", async () => {
		const mediaStreamLinkFactory = vi.fn(async () => {
			throw new Error("session failed");
		});

		render(
			<AudioPreview
				file={{ name: "track.mp3", mime_type: "audio/mpeg" }}
				path="/s/share-token/download"
				mediaStreamLinkFactory={mediaStreamLinkFactory}
			/>,
		);

		fireEvent.click(screen.getByRole("button", { name: "audio_preview_play" }));

		expect(await screen.findByText("preview_load_failed")).toBeInTheDocument();
		expect(mockState.warn).toHaveBeenCalledWith(
			"audio stream session creation failed",
			"track.mp3",
			expect.any(Error),
		);
	});

	it("requests playback again when the current track is already loaded", () => {
		mockState.audioStore.track = {
			id: "track.mp3:unknown:audio/mpeg:/files/7/download",
			name: "track.mp3",
			path: "/files/7/download",
		};

		render(
			<AudioPreview
				file={{ name: "track.mp3", mime_type: "audio/mpeg" }}
				path="/files/7/download"
			/>,
		);

		fireEvent.click(
			screen.getByRole("button", { name: "audio_preview_resume" }),
		);

		expect(mockState.audioStore.requestPlayback).toHaveBeenCalledTimes(1);
		expect(mockState.playTrack).not.toHaveBeenCalled();
	});

	it("does not start playback after the preview unmounts during stream creation", async () => {
		let resolveSession:
			| ((link: { expires_at: string; path: string }) => void)
			| undefined;
		const mediaStreamLinkFactory = vi.fn(
			() =>
				new Promise<{
					expires_at: string;
					path: string;
				}>((resolve) => {
					resolveSession = resolve;
				}),
		);

		const { unmount } = render(
			<AudioPreview
				file={{ name: "track.mp3", mime_type: "audio/mpeg" }}
				path="/s/share-token/download"
				mediaStreamLinkFactory={mediaStreamLinkFactory}
			/>,
		);

		fireEvent.click(screen.getByRole("button", { name: "audio_preview_play" }));
		unmount();
		resolveSession?.({
			expires_at: "2026-01-01T00:00:00Z",
			path: "/api/v1/s/share-token/stream/session-token/track.mp3",
		});

		await waitFor(() => {
			expect(mockState.playTrack).not.toHaveBeenCalled();
		});
	});

	it("shows the playing state for the current track and prevents duplicate starts", () => {
		mockState.audioStore.isPlaying = true;
		mockState.audioStore.track = {
			id: "track.mp3:unknown:audio/mpeg:/files/7/download",
			name: "track.mp3",
			path: "/files/7/download",
		};

		render(
			<AudioPreview
				file={{ name: "track.mp3", mime_type: "audio/mpeg" }}
				path="/files/7/download"
			/>,
		);

		const playingButton = screen.getByRole("button", {
			name: "audio_preview_playing",
		});
		expect(screen.getAllByText("audio_preview_playing")).toHaveLength(2);
		expect(playingButton).toBeDisabled();
		fireEvent.click(playingButton);

		expect(mockState.audioStore.requestPlayback).not.toHaveBeenCalled();
		expect(mockState.playTrack).not.toHaveBeenCalled();
	});
});
