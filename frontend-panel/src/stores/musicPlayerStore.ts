import { create } from "zustand";
import type { FileCategory, ShareStreamSessionInfo } from "@/types/api";

export type MusicPlaybackMode = "repeat_queue" | "repeat_one" | "shuffle";

export interface MusicTrackMetadata {
	album?: string | null;
	artist?: string | null;
	artists?: string[] | null;
	artworkUrl?: string | null;
	title?: string | null;
}

export interface MusicTrackThumbnailSource {
	file: {
		id: number;
		file_category?: FileCategory;
		mime_type: string;
		name: string;
	};
	path?: string;
}

export interface MusicPlayerTrack {
	id: string;
	name: string;
	mimeType: string;
	path: string;
	size?: number;
	expiresAt?: string;
	metadata?: MusicTrackMetadata;
	thumbnail?: MusicTrackThumbnailSource;
	loadBackendMetadata?: (
		signal?: AbortSignal,
	) => Promise<MusicTrackMetadata | null>;
	refreshStreamLink?: () => Promise<ShareStreamSessionInfo>;
}

interface MusicPlayerState {
	activeTrackId: string | null;
	error: string | null;
	isPanelOpen: boolean;
	isPlaying: boolean;
	playRequestVersion: number;
	playRequested: boolean;
	playbackMode: MusicPlaybackMode;
	queue: MusicPlayerTrack[];
	clear: () => void;
	closePanel: () => void;
	openPanel: () => void;
	playNext: () => void;
	playPrevious: () => void;
	playTrack: (track: MusicPlayerTrack) => void;
	playTracks: (tracks: MusicPlayerTrack[], activeTrackId: string) => void;
	requestPlayback: () => void;
	setError: (error: string | null) => void;
	setPanelOpen: (isPanelOpen: boolean) => void;
	setPlaybackMode: (mode: MusicPlaybackMode) => void;
	setPlaying: (isPlaying: boolean) => void;
	setPlaybackRequested: (playRequested: boolean) => void;
	togglePanel: () => void;
	updateTrackMetadata: (
		trackId: string,
		metadata: Partial<MusicTrackMetadata>,
	) => void;
	updateTrackSource: (
		trackId: string,
		link: Pick<ShareStreamSessionInfo, "expires_at" | "path">,
	) => void;
}

function uniqueTracks(tracks: MusicPlayerTrack[]) {
	const seen = new Set<string>();
	const result: MusicPlayerTrack[] = [];

	for (const track of tracks) {
		if (seen.has(track.id)) {
			continue;
		}
		seen.add(track.id);
		result.push(track);
	}

	return result;
}

function nextTrackId(
	queue: MusicPlayerTrack[],
	activeTrackId: string | null,
	mode: MusicPlaybackMode,
) {
	if (queue.length === 0) return null;
	if (mode === "shuffle" && queue.length > 1) {
		const currentIndex = queue.findIndex((track) => track.id === activeTrackId);
		const candidates = queue.filter((_, index) => index !== currentIndex);
		const randomIndex = Math.floor(Math.random() * candidates.length);
		return candidates[randomIndex]?.id ?? queue[0]?.id ?? null;
	}

	const index = queue.findIndex((track) => track.id === activeTrackId);
	if (index < 0) return queue[0]?.id ?? null;
	return queue[(index + 1) % queue.length]?.id ?? null;
}

function previousTrackId(
	queue: MusicPlayerTrack[],
	activeTrackId: string | null,
	mode: MusicPlaybackMode,
) {
	if (queue.length === 0) return null;
	if (mode === "shuffle" && queue.length > 1) {
		return nextTrackId(queue, activeTrackId, mode);
	}

	const index = queue.findIndex((track) => track.id === activeTrackId);
	if (index < 0) return queue[0]?.id ?? null;
	return queue[(index - 1 + queue.length) % queue.length]?.id ?? null;
}

export const useMusicPlayerStore = create<MusicPlayerState>((set) => ({
	activeTrackId: null,
	error: null,
	isPanelOpen: false,
	isPlaying: false,
	playRequestVersion: 0,
	playRequested: false,
	playbackMode: "repeat_queue",
	queue: [],

	clear: () =>
		set({
			activeTrackId: null,
			error: null,
			isPanelOpen: false,
			isPlaying: false,
			playRequestVersion: 0,
			playRequested: false,
			queue: [],
		}),

	closePanel: () => set({ isPanelOpen: false }),
	openPanel: () => set({ isPanelOpen: true }),

	playNext: () =>
		set((state) => {
			const activeTrackId = nextTrackId(
				state.queue,
				state.activeTrackId,
				state.playbackMode,
			);
			if (!activeTrackId) {
				return {
					activeTrackId: null,
					isPlaying: false,
					playRequested: false,
				};
			}

			return {
				activeTrackId,
				error: null,
				playRequestVersion: state.playRequestVersion + 1,
				playRequested: true,
			};
		}),

	playPrevious: () =>
		set((state) => {
			const activeTrackId = previousTrackId(
				state.queue,
				state.activeTrackId,
				state.playbackMode,
			);
			if (!activeTrackId) {
				return state;
			}

			return {
				activeTrackId,
				error: null,
				playRequestVersion: state.playRequestVersion + 1,
				playRequested: true,
			};
		}),

	playTrack: (track) =>
		set((state) => {
			const existingIndex = state.queue.findIndex(
				(candidate) => candidate.id === track.id,
			);
			const queue =
				existingIndex >= 0
					? state.queue.map((candidate, index) =>
							index === existingIndex ? track : candidate,
						)
					: [track];

			return {
				activeTrackId: track.id,
				error: null,
				playRequestVersion: state.playRequestVersion + 1,
				playRequested: true,
				queue,
			};
		}),

	playTracks: (tracks, activeTrackId) =>
		set((state) => {
			const queue = uniqueTracks(tracks);
			const activeTrack = queue.find((track) => track.id === activeTrackId);

			if (!activeTrack) {
				return state;
			}

			return {
				activeTrackId: activeTrack.id,
				error: null,
				playRequestVersion: state.playRequestVersion + 1,
				playRequested: true,
				queue,
			};
		}),

	requestPlayback: () =>
		set((state) => ({
			playRequestVersion: state.playRequestVersion + 1,
			playRequested: true,
		})),

	setError: (error) => set({ error }),
	setPanelOpen: (isPanelOpen) => set({ isPanelOpen }),
	setPlaybackMode: (playbackMode) => set({ playbackMode }),
	setPlaying: (isPlaying) => set({ isPlaying }),
	setPlaybackRequested: (playRequested) => set({ playRequested }),
	togglePanel: () => set((state) => ({ isPanelOpen: !state.isPanelOpen })),

	updateTrackMetadata: (trackId, metadata) =>
		set((state) => {
			if (!state.queue.some((track) => track.id === trackId)) {
				return state;
			}

			return {
				queue: state.queue.map((track) =>
					track.id === trackId
						? {
								...track,
								metadata: {
									...(track.metadata ?? {}),
									...metadata,
								},
							}
						: track,
				),
			};
		}),

	updateTrackSource: (trackId, link) =>
		set((state) => {
			if (!state.queue.some((track) => track.id === trackId)) {
				return state;
			}

			return {
				queue: state.queue.map((track) =>
					track.id === trackId
						? {
								...track,
								expiresAt: link.expires_at,
								path: link.path,
							}
						: track,
				),
			};
		}),
}));
