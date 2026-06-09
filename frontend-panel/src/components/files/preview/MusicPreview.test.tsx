import {
	act,
	fireEvent,
	render,
	screen,
	waitFor,
} from "@testing-library/react";
import { beforeEach, describe, expect, it, vi } from "vitest";
import { MusicPreview } from "@/components/files/preview/MusicPreview";

const mockState = vi.hoisted(() => ({
	musicStore: {
		activeTrackId: null as string | null,
		isPlaying: false,
		queue: [] as Array<{
			id: string;
			name: string;
			path: string;
		}>,
		requestPlayback: vi.fn(),
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

vi.mock("@/stores/musicPlayerStore", () => ({
	useMusicPlayerStore: (
		selector: (state: {
			activeTrackId: string | null;
			isPlaying: boolean;
			playTrack: (...args: unknown[]) => void;
			queue: typeof mockState.musicStore.queue;
			requestPlayback: (...args: unknown[]) => void;
		}) => unknown,
	) =>
		selector({
			activeTrackId: mockState.musicStore.activeTrackId,
			isPlaying: mockState.musicStore.isPlaying,
			playTrack: (...args: unknown[]) => mockState.playTrack(...args),
			queue: mockState.musicStore.queue,
			requestPlayback: (...args: unknown[]) =>
				mockState.musicStore.requestPlayback(...args),
		}),
}));

describe("MusicPreview", () => {
	beforeEach(() => {
		mockState.musicStore.activeTrackId = null;
		mockState.musicStore.isPlaying = false;
		mockState.musicStore.queue = [];
		mockState.musicStore.requestPlayback.mockReset();
		mockState.playTrack.mockReset();
		mockState.warn.mockReset();
	});

	it("renders a playback entry point without creating a media element", () => {
		render(
			<MusicPreview
				file={{ name: "track.mp3", mime_type: "audio/mpeg" }}
				path="/files/7/download"
			/>,
		);

		expect(document.querySelector("audio")).toBeNull();
		expect(screen.queryByText("track.mp3")).not.toBeInTheDocument();
		expect(screen.getByText("music_preview_idle")).toBeInTheDocument();
		expect(
			screen.getByRole("button", { name: "music_preview_play" }),
		).toBeInTheDocument();
	});

	it("starts direct music playback through the global player store", async () => {
		render(
			<MusicPreview
				file={{ id: 7, name: "track.mp3", mime_type: "audio/mpeg" }}
				path="/files/7/download"
				thumbnailPath="/files/7/thumbnail"
			/>,
		);

		fireEvent.click(screen.getByRole("button", { name: "music_preview_play" }));

		await waitFor(() => {
			expect(mockState.playTrack).toHaveBeenCalledWith(
				expect.objectContaining({
					metadata: { title: "track" },
					name: "track.mp3",
					path: "/files/7/download",
					thumbnail: {
						file: {
							file_category: "audio",
							id: 7,
							mime_type: "audio/mpeg",
							name: "track.mp3",
						},
						path: "/files/7/thumbnail",
					},
				}),
			);
		});
	});

	it("uses a stable track id including file size to avoid resuming a different file", async () => {
		render(
			<MusicPreview
				file={{ name: "track.mp3", mime_type: "audio/mpeg", size: 4096 }}
				path="/files/7/download"
			/>,
		);

		fireEvent.click(screen.getByRole("button", { name: "music_preview_play" }));

		await waitFor(() => {
			expect(mockState.playTrack).toHaveBeenCalledWith(
				expect.objectContaining({
					id: "track.mp3:4096:audio/mpeg:/files/7/download",
					size: 4096,
				}),
			);
		});
	});

	it("creates a stream session before starting share music playback", async () => {
		const mediaStreamLinkFactory = vi.fn(async () => ({
			expires_at: "2026-01-01T00:00:00Z",
			path: "/api/v1/s/share-token/stream/session-token/track.mp3",
		}));

		render(
			<MusicPreview
				file={{ name: "track.mp3", mime_type: "audio/mpeg" }}
				path="/s/share-token/download"
				mediaStreamLinkFactory={mediaStreamLinkFactory}
			/>,
		);

		fireEvent.click(screen.getByRole("button", { name: "music_preview_play" }));

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
			<MusicPreview
				file={{ name: "track.mp3", mime_type: "audio/mpeg" }}
				path="/s/share-token/download"
				mediaStreamLinkFactory={mediaStreamLinkFactory}
			/>,
		);

		fireEvent.click(screen.getByRole("button", { name: "music_preview_play" }));

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
			<MusicPreview
				file={{ name: "track.mp3", mime_type: "audio/mpeg" }}
				path="/s/share-token/download"
				mediaStreamLinkFactory={mediaStreamLinkFactory}
			/>,
		);

		fireEvent.click(screen.getByRole("button", { name: "music_preview_play" }));

		expect(await screen.findByText("preview_load_failed")).toBeInTheDocument();
		expect(mockState.warn).toHaveBeenCalledWith(
			"audio stream session creation failed",
			"track.mp3",
			expect.any(Error),
		);
	});

	it("requests playback again when the current track is already loaded", () => {
		mockState.musicStore.activeTrackId =
			"track.mp3:unknown:audio/mpeg:/files/7/download";
		mockState.musicStore.queue = [
			{
				id: "track.mp3:unknown:audio/mpeg:/files/7/download",
				name: "track.mp3",
				path: "/files/7/download",
			},
		];

		render(
			<MusicPreview
				file={{ name: "track.mp3", mime_type: "audio/mpeg" }}
				path="/files/7/download"
			/>,
		);

		fireEvent.click(
			screen.getByRole("button", { name: "music_preview_resume" }),
		);

		expect(mockState.musicStore.requestPlayback).toHaveBeenCalledTimes(1);
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
			<MusicPreview
				file={{ name: "track.mp3", mime_type: "audio/mpeg" }}
				path="/s/share-token/download"
				mediaStreamLinkFactory={mediaStreamLinkFactory}
			/>,
		);

		fireEvent.click(screen.getByRole("button", { name: "music_preview_play" }));
		unmount();
		resolveSession?.({
			expires_at: "2026-01-01T00:00:00Z",
			path: "/api/v1/s/share-token/stream/session-token/track.mp3",
		});

		await waitFor(() => {
			expect(mockState.playTrack).not.toHaveBeenCalled();
		});
	});

	it("does not show a stream error after the preview unmounts during a failed start", async () => {
		let rejectSession: ((error: Error) => void) | undefined;
		const mediaStreamLinkFactory = vi.fn(
			() =>
				new Promise<{
					expires_at: string;
					path: string;
				}>((_, reject) => {
					rejectSession = reject;
				}),
		);

		const { unmount } = render(
			<MusicPreview
				file={{ name: "track.mp3", mime_type: "audio/mpeg" }}
				path="/s/share-token/download"
				mediaStreamLinkFactory={mediaStreamLinkFactory}
			/>,
		);

		fireEvent.click(screen.getByRole("button", { name: "music_preview_play" }));
		await waitFor(() => {
			expect(mediaStreamLinkFactory).toHaveBeenCalledTimes(1);
		});
		unmount();
		await act(async () => {
			rejectSession?.(new Error("unmounted failure"));
		});

		expect(mockState.playTrack).not.toHaveBeenCalled();
		expect(mockState.warn).not.toHaveBeenCalled();
		expect(screen.queryByText("preview_load_failed")).not.toBeInTheDocument();
	});

	it("shows the playing state for the current track and prevents duplicate starts", () => {
		mockState.musicStore.activeTrackId =
			"track.mp3:unknown:audio/mpeg:/files/7/download";
		mockState.musicStore.isPlaying = true;
		mockState.musicStore.queue = [
			{
				id: "track.mp3:unknown:audio/mpeg:/files/7/download",
				name: "track.mp3",
				path: "/files/7/download",
			},
		];

		render(
			<MusicPreview
				file={{ name: "track.mp3", mime_type: "audio/mpeg" }}
				path="/files/7/download"
			/>,
		);

		const playingButton = screen.getByRole("button", {
			name: "music_preview_playing",
		});
		expect(screen.getAllByText("music_preview_playing")).toHaveLength(2);
		expect(playingButton).toBeDisabled();
		fireEvent.click(playingButton);

		expect(mockState.musicStore.requestPlayback).not.toHaveBeenCalled();
		expect(mockState.playTrack).not.toHaveBeenCalled();
	});
});
