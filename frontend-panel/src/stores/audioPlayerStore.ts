import { create } from "zustand";
import type { ShareStreamSessionInfo } from "@/types/api";

export interface AudioPlayerTrack {
	id: string;
	name: string;
	mimeType: string;
	path: string;
	size?: number;
	expiresAt?: string;
	refreshStreamLink?: () => Promise<ShareStreamSessionInfo>;
}

interface AudioPlayerState {
	error: string | null;
	isPlaying: boolean;
	playRequestVersion: number;
	playRequested: boolean;
	track: AudioPlayerTrack | null;
	clear: () => void;
	playTrack: (track: AudioPlayerTrack) => void;
	requestPlayback: () => void;
	setError: (error: string | null) => void;
	setPlaying: (isPlaying: boolean) => void;
	setPlaybackRequested: (playRequested: boolean) => void;
	updateTrackSource: (
		trackId: string,
		link: Pick<ShareStreamSessionInfo, "expires_at" | "path">,
	) => void;
}

export const useAudioPlayerStore = create<AudioPlayerState>((set) => ({
	error: null,
	isPlaying: false,
	playRequestVersion: 0,
	playRequested: false,
	track: null,

	clear: () =>
		set({
			error: null,
			isPlaying: false,
			playRequestVersion: 0,
			playRequested: false,
			track: null,
		}),

	playTrack: (track) =>
		set((state) => ({
			error: null,
			playRequestVersion: state.playRequestVersion + 1,
			playRequested: true,
			track,
		})),

	requestPlayback: () =>
		set((state) => ({
			playRequestVersion: state.playRequestVersion + 1,
			playRequested: true,
		})),

	setError: (error) => set({ error }),
	setPlaying: (isPlaying) => set({ isPlaying }),
	setPlaybackRequested: (playRequested) => set({ playRequested }),

	updateTrackSource: (trackId, link) =>
		set((state) => {
			if (!state.track || state.track.id !== trackId) {
				return state;
			}

			return {
				track: {
					...state.track,
					expiresAt: link.expires_at,
					path: link.path,
				},
			};
		}),
}));
