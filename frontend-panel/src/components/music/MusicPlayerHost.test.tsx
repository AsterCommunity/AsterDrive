import {
	act,
	fireEvent,
	render,
	screen,
	waitFor,
} from "@testing-library/react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { MusicPlayerHost } from "@/components/music/MusicPlayerHost";
import { ApiPendingError } from "@/services/http";
import type { MusicPlaybackMode } from "@/stores/musicPlayerStore";

type MockMediaSession = {
	metadata: MediaMetadata | null;
	playbackState: MediaSessionPlaybackState;
	setActionHandler: ReturnType<typeof vi.fn>;
	setPositionState: ReturnType<typeof vi.fn>;
};

const mockState = vi.hoisted(() => ({
	clear: vi.fn(),
	closePanel: vi.fn(),
	parseMusicMetadataFromSource: vi.fn(),
	openPanel: vi.fn(),
	playNext: vi.fn(),
	playPrevious: vi.fn(),
	playTracks: vi.fn(),
	requestPlayback: vi.fn(),
	setError: vi.fn(),
	setPanelOpen: vi.fn(),
	setPlaybackMode: vi.fn(),
	setPlaybackRequested: vi.fn(),
	setPlaying: vi.fn(),
	updateTrackSource: vi.fn(),
	updateTrackMetadata: vi.fn(),
	thumbnailSupportStore: {
		config: {
			version: 1,
			extensions: ["mp3"],
		},
		invalidate: vi.fn(),
		isLoaded: true,
		load: vi.fn(),
	},
	useBlobUrl: vi.fn(),
	state: {
		activeTrackId: null as string | null,
		error: null as string | null,
		isPanelOpen: false,
		isPlaying: false,
		playRequestVersion: 0,
		playRequested: false,
		playbackMode: "repeat_queue" as MusicPlaybackMode,
		queue: [] as Array<{
			expiresAt?: string;
			refreshStreamLink?: () => Promise<{
				expires_at: string;
				path: string;
			}>;
			id: string;
			metadata?: {
				album?: string | null;
				artist?: string | null;
				artists?: string[] | null;
				artworkUrl?: string | null;
				title?: string | null;
			};
			mimeType: string;
			name: string;
			path: string;
			size?: number;
			thumbnail?: {
				file: {
					file_category?: "audio";
					id: number;
					mime_type: string;
					name: string;
				};
				path?: string;
			};
		}>,
	},
}));

vi.mock("react-i18next", () => ({
	useTranslation: () => ({
		t: (key: string) => key,
	}),
}));

vi.mock("@/stores/musicPlayerStore", () => ({
	useMusicPlayerStore: (
		selector: (state: {
			activeTrackId: typeof mockState.state.activeTrackId;
			clear: typeof mockState.clear;
			closePanel: typeof mockState.closePanel;
			error: string | null;
			isPanelOpen: boolean;
			isPlaying: boolean;
			openPanel: typeof mockState.openPanel;
			playNext: typeof mockState.playNext;
			playPrevious: typeof mockState.playPrevious;
			playRequestVersion: number;
			playRequested: boolean;
			playbackMode: MusicPlaybackMode;
			playTracks: typeof mockState.playTracks;
			queue: typeof mockState.state.queue;
			requestPlayback: typeof mockState.requestPlayback;
			setError: typeof mockState.setError;
			setPanelOpen: typeof mockState.setPanelOpen;
			setPlaybackMode: typeof mockState.setPlaybackMode;
			setPlaybackRequested: typeof mockState.setPlaybackRequested;
			setPlaying: typeof mockState.setPlaying;
			updateTrackMetadata: typeof mockState.updateTrackMetadata;
			updateTrackSource: typeof mockState.updateTrackSource;
		}) => unknown,
	) =>
		selector({
			...mockState.state,
			clear: mockState.clear,
			closePanel: mockState.closePanel,
			openPanel: mockState.openPanel,
			playNext: mockState.playNext,
			playPrevious: mockState.playPrevious,
			playTracks: mockState.playTracks,
			requestPlayback: mockState.requestPlayback,
			setError: mockState.setError,
			setPanelOpen: mockState.setPanelOpen,
			setPlaybackMode: mockState.setPlaybackMode,
			setPlaybackRequested: mockState.setPlaybackRequested,
			setPlaying: mockState.setPlaying,
			updateTrackMetadata: mockState.updateTrackMetadata,
			updateTrackSource: mockState.updateTrackSource,
		}),
}));

vi.mock("@/lib/musicPlayer", () => ({
	parseMusicMetadataFromSource: (...args: unknown[]) =>
		mockState.parseMusicMetadataFromSource(...args),
}));

vi.mock("@/hooks/useBlobUrl", () => ({
	useBlobUrl: (...args: unknown[]) => mockState.useBlobUrl(...args),
}));

vi.mock("@/stores/thumbnailSupportStore", () => ({
	useThumbnailSupportStore: (
		selector: (state: typeof mockState.thumbnailSupportStore) => unknown,
	) => selector(mockState.thumbnailSupportStore),
}));

vi.mock("@/components/files/FileThumbnail", () => ({
	FileThumbnail: ({
		className,
		file,
		thumbnailPath,
	}: {
		className?: string;
		file: {
			id: number;
			name: string;
		};
		thumbnailPath?: string;
	}) => (
		<div
			className={className}
			data-file-id={file.id}
			data-file-name={file.name}
			data-testid="file-thumbnail"
			data-thumbnail-path={thumbnailPath ?? ""}
		/>
	),
}));

function setQueue() {
	mockState.state.activeTrackId = "track-1";
	mockState.state.queue = [
		{
			id: "track-1",
			metadata: { artist: "Artist One", title: "Track One" },
			mimeType: "audio/mpeg",
			name: "track-one.mp3",
			path: "/files/7/download",
			size: 1024,
			thumbnail: {
				file: {
					file_category: "audio",
					id: 7,
					mime_type: "audio/mpeg",
					name: "track-one.mp3",
				},
				path: "/files/7/thumbnail",
			},
		},
		{
			id: "track-2",
			metadata: { artist: "Artist Two", title: "Track Two" },
			mimeType: "audio/mpeg",
			name: "track-two.mp3",
			path: "/files/8/download",
			size: 2048,
			thumbnail: {
				file: {
					file_category: "audio",
					id: 8,
					mime_type: "audio/mpeg",
					name: "track-two.mp3",
				},
				path: "/files/8/thumbnail",
			},
		},
	];
}

function getQueuedTracks() {
	const [firstTrack, secondTrack] = mockState.state.queue;
	if (!firstTrack || !secondTrack) {
		throw new Error("expected queued test tracks");
	}
	return { firstTrack, secondTrack };
}

function getPlayerPanel() {
	return screen.getByRole("region", {
		hidden: true,
		name: "music_player_title",
	}).parentElement;
}

function installMockMediaSession() {
	const handlers = new Map<MediaSessionAction, MediaSessionActionHandler>();
	const mediaSession: MockMediaSession = {
		metadata: null,
		playbackState: "none",
		setActionHandler: vi.fn(
			(
				action: MediaSessionAction,
				handler: MediaSessionActionHandler | null,
			) => {
				if (handler) {
					handlers.set(action, handler);
					return;
				}
				handlers.delete(action);
			},
		),
		setPositionState: vi.fn(),
	};

	class MockMediaMetadata {
		album: string;
		artist: string;
		artwork: ReadonlyArray<MediaImage>;
		title: string;

		constructor(init: MediaMetadataInit = {}) {
			this.album = init.album ?? "";
			this.artist = init.artist ?? "";
			this.artwork = init.artwork ?? [];
			this.title = init.title ?? "";
		}
	}

	Object.defineProperty(window, "MediaMetadata", {
		configurable: true,
		value: MockMediaMetadata,
		writable: true,
	});
	Object.defineProperty(navigator, "mediaSession", {
		configurable: true,
		value: mediaSession,
	});

	return {
		fireAction(action: MediaSessionAction, details = {}) {
			handlers.get(action)?.({
				action,
				...details,
			} as MediaSessionActionDetails);
		},
		handlers,
		mediaSession,
	};
}

function mockOverflow(text: string, scrollWidth: number, clientWidth: number) {
	const textNode = [...document.querySelectorAll("*")]
		.reverse()
		.find(
			(element) =>
				element.textContent === text && element.children.length === 0,
		);
	if (!textNode) {
		throw new Error(`missing text node: ${text}`);
	}
	const viewport = textNode.parentElement;
	if (!viewport) {
		throw new Error(`missing viewport for text node: ${text}`);
	}
	Object.defineProperty(textNode, "scrollWidth", {
		configurable: true,
		value: scrollWidth,
	});
	Object.defineProperty(viewport, "clientWidth", {
		configurable: true,
		value: clientWidth,
	});
	window.dispatchEvent(new Event("resize"));
	return { textNode, viewport };
}

function installScrollIntoViewMock() {
	const scrollIntoView = vi.fn();
	HTMLElement.prototype.scrollIntoView = scrollIntoView;
	return scrollIntoView;
}

async function flushAsyncEffects() {
	await act(async () => {
		for (let index = 0; index < 6; index += 1) {
			await Promise.resolve();
		}
	});
}

describe("MusicPlayerHost", () => {
	let originalMediaMetadata: PropertyDescriptor | undefined;
	let originalMediaSession: PropertyDescriptor | undefined;
	let originalResizeObserver: typeof ResizeObserver | undefined;
	let originalScrollIntoView: PropertyDescriptor | undefined;

	beforeEach(() => {
		vi.useRealTimers();
		originalMediaMetadata = Object.getOwnPropertyDescriptor(
			window,
			"MediaMetadata",
		);
		originalMediaSession = Object.getOwnPropertyDescriptor(
			navigator,
			"mediaSession",
		);
		originalResizeObserver = window.ResizeObserver;
		originalScrollIntoView = Object.getOwnPropertyDescriptor(
			HTMLElement.prototype,
			"scrollIntoView",
		);
		Object.defineProperty(window, "requestAnimationFrame", {
			configurable: true,
			value: window.requestAnimationFrame ?? vi.fn(),
			writable: true,
		});
		Object.defineProperty(window, "cancelAnimationFrame", {
			configurable: true,
			value: window.cancelAnimationFrame ?? vi.fn(),
			writable: true,
		});
		class MockResizeObserver {
			callback: ResizeObserverCallback;

			constructor(callback: ResizeObserverCallback) {
				this.callback = callback;
			}

			disconnect = vi.fn();
			observe = vi.fn(() => {
				this.callback([], this as unknown as ResizeObserver);
			});
			unobserve = vi.fn();
		}
		window.ResizeObserver =
			MockResizeObserver as unknown as typeof ResizeObserver;
		if (typeof window.requestAnimationFrame !== "function") {
			Object.defineProperty(window, "requestAnimationFrame", {
				configurable: true,
				value: (_callback: FrameRequestCallback) => 1,
				writable: true,
			});
		}
		if (typeof window.cancelAnimationFrame !== "function") {
			Object.defineProperty(window, "cancelAnimationFrame", {
				configurable: true,
				value: () => undefined,
				writable: true,
			});
		}
		vi.spyOn(window, "requestAnimationFrame").mockImplementation((callback) => {
			callback(0);
			return 1;
		});
		vi.spyOn(window, "cancelAnimationFrame").mockImplementation(() => {});
		Object.defineProperty(HTMLMediaElement.prototype, "play", {
			configurable: true,
			value: vi.fn(() => Promise.resolve()),
		});
		Object.defineProperty(HTMLMediaElement.prototype, "pause", {
			configurable: true,
			value: vi.fn(),
		});
		Object.defineProperty(HTMLMediaElement.prototype, "fastSeek", {
			configurable: true,
			value: vi.fn(function fastSeek(this: HTMLMediaElement, time: number) {
				this.currentTime = time;
			}),
		});
		mockState.clear.mockReset();
		mockState.closePanel.mockReset();
		mockState.openPanel.mockReset();
		mockState.parseMusicMetadataFromSource.mockReset();
		mockState.parseMusicMetadataFromSource.mockResolvedValue({
			artist: "Parsed Artist",
			title: "Parsed Title",
		});
		mockState.playNext.mockReset();
		mockState.playPrevious.mockReset();
		mockState.playTracks.mockReset();
		mockState.requestPlayback.mockReset();
		mockState.setError.mockReset();
		mockState.setError.mockImplementation((error: string | null) => {
			mockState.state.error = error;
		});
		mockState.setPanelOpen.mockReset();
		mockState.setPlaybackMode.mockReset();
		mockState.setPlaybackRequested.mockReset();
		mockState.setPlaying.mockReset();
		mockState.updateTrackMetadata.mockReset();
		mockState.updateTrackMetadata.mockImplementation(
			(trackId: string, metadata: Record<string, unknown>) => {
				mockState.state.queue = mockState.state.queue.map((track) =>
					track.id === trackId
						? {
								...track,
								metadata: {
									...(track.metadata ?? {}),
									...metadata,
								},
							}
						: track,
				);
			},
		);
		mockState.updateTrackSource.mockReset();
		mockState.thumbnailSupportStore.config = {
			version: 1,
			extensions: ["mp3"],
		};
		mockState.thumbnailSupportStore.isLoaded = true;
		mockState.thumbnailSupportStore.invalidate.mockReset();
		mockState.thumbnailSupportStore.load.mockReset();
		mockState.useBlobUrl.mockReset();
		mockState.useBlobUrl.mockReturnValue({
			blob: null,
			blobUrl: null,
			error: false,
			loading: false,
			retry: vi.fn(),
		});
		mockState.closePanel.mockImplementation(() => {
			mockState.state.isPanelOpen = false;
		});
		mockState.state.activeTrackId = null;
		mockState.state.error = null;
		mockState.state.isPanelOpen = false;
		mockState.state.isPlaying = false;
		mockState.state.playRequestVersion = 0;
		mockState.state.playRequested = false;
		mockState.state.playbackMode = "repeat_queue";
		mockState.state.queue = [];
	});

	afterEach(() => {
		if (originalMediaMetadata) {
			Object.defineProperty(window, "MediaMetadata", originalMediaMetadata);
		} else {
			Reflect.deleteProperty(window, "MediaMetadata");
		}
		if (originalMediaSession) {
			Object.defineProperty(navigator, "mediaSession", originalMediaSession);
		} else {
			Reflect.deleteProperty(navigator, "mediaSession");
		}
		window.ResizeObserver = originalResizeObserver;
		if (originalScrollIntoView) {
			Object.defineProperty(
				HTMLElement.prototype,
				"scrollIntoView",
				originalScrollIntoView,
			);
		} else {
			Reflect.deleteProperty(HTMLElement.prototype, "scrollIntoView");
		}
		vi.restoreAllMocks();
		vi.useRealTimers();
	});

	it("renders nothing when no track is loaded", () => {
		const { container } = render(<MusicPlayerHost />);

		expect(container).toBeEmptyDOMElement();
	});

	it("keeps the audio element loaded while the panel is collapsed", () => {
		setQueue();

		render(<MusicPlayerHost />);

		expect(document.querySelector("audio")).toHaveAttribute(
			"src",
			"/api/v1/files/7/download",
		);
		expect(getPlayerPanel()).toHaveAttribute("data-state", "closed");
		expect(getPlayerPanel()).toHaveAttribute("inert");
	});

	it("renders the expanded player with track metadata while keeping the queue collapsed by default", () => {
		setQueue();
		mockState.state.isPanelOpen = true;

		render(<MusicPlayerHost />);

		const queueToggle = screen.getByRole("button", {
			name: /music_player_queue/i,
		});

		expect(getPlayerPanel()).toHaveAttribute("data-state", "open");
		expect(getPlayerPanel()).not.toHaveAttribute("inert");
		expect(screen.getByText("music_player_title")).toBeInTheDocument();
		expect(screen.getByText("Track One")).toBeInTheDocument();
		expect(screen.getByText("Artist One")).toBeInTheDocument();
		expect(screen.getByTestId("file-thumbnail")).toHaveAttribute(
			"data-thumbnail-path",
			"/files/7/thumbnail",
		);
		expect(queueToggle).toHaveAttribute("aria-expanded", "false");
		expect(screen.queryByText("Track Two")).not.toBeInTheDocument();

		fireEvent.click(queueToggle);

		expect(queueToggle).toHaveAttribute("aria-expanded", "true");
		expect(screen.getByText("Track Two")).toBeInTheDocument();
		expect(
			screen
				.getAllByTestId("file-thumbnail")
				.map((node) => node.getAttribute("data-thumbnail-path")),
		).toEqual([
			"/files/7/thumbnail",
			"/files/7/thumbnail",
			"/files/8/thumbnail",
		]);
	});

	it("shows file details without repeating the playback mode", () => {
		setQueue();
		mockState.state.isPanelOpen = true;
		const { firstTrack, secondTrack } = getQueuedTracks();
		mockState.state.queue = [
			{
				...firstTrack,
				metadata: {
					...firstTrack.metadata,
					album: "Album One",
				},
			},
			secondTrack,
		];

		render(<MusicPlayerHost />);

		fireEvent.click(
			screen.getByRole("button", { name: /music_player_details/i }),
		);

		expect(screen.getByText("music_player_file_name")).toBeInTheDocument();
		expect(screen.getByText("track-one.mp3")).toBeInTheDocument();
		expect(screen.getByText("music_player_title_label")).toBeInTheDocument();
		expect(screen.getAllByText("Track One").length).toBeGreaterThan(0);
		expect(screen.getByText("music_player_artist_label")).toBeInTheDocument();
		expect(screen.getAllByText("Artist One").length).toBeGreaterThan(0);
		expect(screen.getByText("music_player_album_label")).toBeInTheDocument();
		expect(screen.getByText("Album One")).toBeInTheDocument();
		expect(screen.getByText("music_player_mime_type")).toBeInTheDocument();
		expect(screen.getByText("audio/mpeg")).toBeInTheDocument();
		expect(screen.queryByText("music_player_mode")).not.toBeInTheDocument();
	});

	it("uses automatic marquee only for active overflowing music text", async () => {
		mockState.state.activeTrackId = "track-1";
		mockState.state.isPanelOpen = true;
		mockState.state.queue = [
			{
				id: "track-1",
				metadata: {
					artist: "First Artist",
					artists: ["First Artist", "Second Artist"],
					title:
						"Very Long Track Title That Needs Automatic Scrolling In The Player",
				},
				mimeType: "audio/mpeg",
				name: "track-one.mp3",
				path: "/files/7/download",
				size: 1024,
			},
			{
				id: "track-2",
				metadata: {
					artist: "Other Artist",
					title: "Very Long Inactive Track Title That Must Stay Truncated",
				},
				mimeType: "audio/mpeg",
				name: "track-two.mp3",
				path: "/files/8/download",
				size: 2048,
			},
		];

		render(<MusicPlayerHost />);
		fireEvent.click(
			screen.getByRole("button", { name: /music_player_queue/i }),
		);

		const { viewport: activeViewport } = mockOverflow(
			"Very Long Track Title That Needs Automatic Scrolling In The Player",
			720,
			240,
		);
		await waitFor(() =>
			expect(activeViewport).toHaveAttribute("data-marquee-active", "true"),
		);

		const scrollingTrack = activeViewport.querySelector("span[style]");
		if (!scrollingTrack) {
			throw new Error("scrolling track not found");
		}
		expect(scrollingTrack).toHaveStyle({
			"--music-text-scroll-distance": "-744px",
		});
		expect(scrollingTrack).toHaveStyle({
			animation: "music-player-text-marquee 28s linear infinite",
		});
		const { viewport: activeArtistViewport } = mockOverflow(
			"First Artist, Second Artist",
			420,
			180,
		);
		await waitFor(() =>
			expect(activeArtistViewport).toHaveAttribute(
				"data-marquee-active",
				"true",
			),
		);
		const marqueeStyles = [...document.querySelectorAll("style")].filter(
			(style) =>
				style.textContent?.includes("@keyframes music-player-text-marquee"),
		);
		expect(marqueeStyles).toHaveLength(1);
		expect(marqueeStyles[0]?.textContent).toContain("12%");
		expect(marqueeStyles[0]?.textContent).toContain("82%");

		const inactiveTitle = screen.getByText(
			"Very Long Inactive Track Title That Must Stay Truncated",
		);
		expect(inactiveTitle).toHaveClass("truncate");
		expect(inactiveTitle.parentElement).not.toHaveAttribute(
			"data-marquee-active",
			"true",
		);
		expect(
			screen.getAllByText("First Artist, Second Artist").length,
		).toBeGreaterThan(0);
	});

	it("can close or collapse the player panel", () => {
		setQueue();
		mockState.state.isPanelOpen = true;

		render(<MusicPlayerHost />);

		fireEvent.click(screen.getByRole("button", { name: "music_player_close" }));
		fireEvent.click(
			screen.getByRole("button", { name: "music_player_collapse" }),
		);

		expect(mockState.clear).toHaveBeenCalledTimes(1);
		expect(mockState.closePanel).toHaveBeenCalledTimes(1);
	});

	it("collapses the player when clicking outside of it", () => {
		setQueue();
		mockState.state.isPanelOpen = true;

		render(<MusicPlayerHost />);

		fireEvent.click(screen.getByText("Track One"));
		expect(mockState.closePanel).not.toHaveBeenCalled();

		fireEvent.click(document.body);

		expect(mockState.closePanel).toHaveBeenCalledTimes(1);
	});

	it("does not treat queue interactions as outside clicks", () => {
		setQueue();
		mockState.state.isPanelOpen = true;

		render(<MusicPlayerHost />);

		fireEvent.click(
			screen.getByRole("button", { name: /music_player_queue/i }),
		);
		fireEvent.click(screen.getByRole("button", { name: /Track Two/i }));

		expect(mockState.closePanel).not.toHaveBeenCalled();
		expect(mockState.playTracks).toHaveBeenCalledWith(
			mockState.state.queue,
			"track-2",
		);
	});

	it("requests playback when play is clicked", () => {
		setQueue();
		mockState.state.isPanelOpen = true;

		render(<MusicPlayerHost />);

		fireEvent.click(screen.getByRole("button", { name: "music_player_play" }));

		expect(mockState.requestPlayback).toHaveBeenCalledTimes(1);
	});

	it("pauses playback when pause is clicked", () => {
		setQueue();
		mockState.state.isPanelOpen = true;
		mockState.state.isPlaying = true;
		mockState.state.playRequested = true;

		render(<MusicPlayerHost />);

		fireEvent.click(screen.getByRole("button", { name: "music_player_pause" }));

		expect(mockState.setPlaybackRequested).toHaveBeenCalledWith(false);
	});

	it("wires previous, next, playback mode, and queue item actions", () => {
		setQueue();
		mockState.state.isPanelOpen = true;

		render(<MusicPlayerHost />);

		fireEvent.click(
			screen.getByRole("button", { name: "music_player_previous" }),
		);
		fireEvent.click(screen.getByRole("button", { name: "music_player_next" }));
		fireEvent.click(
			screen.getByRole("button", {
				name: "music_player_mode_repeat_queue",
			}),
		);
		fireEvent.click(
			screen.getByRole("button", { name: /music_player_queue/i }),
		);
		fireEvent.click(screen.getByRole("button", { name: /Track Two/i }));

		expect(mockState.playPrevious).toHaveBeenCalledTimes(1);
		expect(mockState.playNext).toHaveBeenCalledTimes(1);
		expect(mockState.setPlaybackMode).toHaveBeenCalledWith("repeat_one");
		expect(mockState.playTracks).toHaveBeenCalledWith(
			mockState.state.queue,
			"track-2",
		);
	});

	it("scrolls the active queue item into view when playback changes outside queue clicks", () => {
		const scrollIntoView = installScrollIntoViewMock();
		setQueue();
		mockState.state.isPanelOpen = true;

		const { rerender } = render(<MusicPlayerHost />);

		fireEvent.click(
			screen.getByRole("button", { name: /music_player_queue/i }),
		);
		scrollIntoView.mockClear();

		mockState.state.activeTrackId = "track-2";
		rerender(<MusicPlayerHost />);

		expect(scrollIntoView).toHaveBeenCalledWith({
			block: "nearest",
			inline: "nearest",
		});
	});

	it("does not auto-scroll the queue after a queue item click changes tracks", () => {
		const scrollIntoView = installScrollIntoViewMock();
		setQueue();
		mockState.state.isPanelOpen = true;

		const { rerender } = render(<MusicPlayerHost />);

		fireEvent.click(
			screen.getByRole("button", { name: /music_player_queue/i }),
		);
		scrollIntoView.mockClear();
		fireEvent.click(screen.getByRole("button", { name: /Track Two/i }));

		mockState.state.activeTrackId = "track-2";
		rerender(<MusicPlayerHost />);

		expect(mockState.playTracks).toHaveBeenCalledWith(
			mockState.state.queue,
			"track-2",
		);
		expect(scrollIntoView).not.toHaveBeenCalled();
	});

	it("reflects audio element events back into the player store", () => {
		setQueue();

		render(<MusicPlayerHost />);

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
		expect(mockState.setError).toHaveBeenCalledWith("music_player_load_failed");
		expect(mockState.setPlaybackRequested).toHaveBeenCalledWith(false);
		expect(mockState.setPlaying).toHaveBeenCalledWith(false);
	});

	it("skips to the next track two seconds after a load error", async () => {
		vi.useFakeTimers();
		setQueue();

		render(<MusicPlayerHost />);

		const audio = document.querySelector("audio");
		if (!audio) {
			throw new Error("audio element not found");
		}

		fireEvent.error(audio);

		expect(mockState.setError).toHaveBeenCalledWith("music_player_load_failed");
		expect(mockState.playNext).not.toHaveBeenCalled();

		await act(async () => {
			vi.advanceTimersByTime(1_999);
		});
		expect(mockState.playNext).not.toHaveBeenCalled();

		await act(async () => {
			vi.advanceTimersByTime(1);
		});

		expect(mockState.playNext).toHaveBeenCalledTimes(1);
	});

	it("cancels a pending error skip when the user manually skips", async () => {
		vi.useFakeTimers();
		setQueue();
		mockState.state.isPanelOpen = true;

		render(<MusicPlayerHost />);

		const audio = document.querySelector("audio");
		if (!audio) {
			throw new Error("audio element not found");
		}

		fireEvent.error(audio);
		fireEvent.click(screen.getByRole("button", { name: "music_player_next" }));

		await act(async () => {
			vi.advanceTimersByTime(2_000);
		});

		expect(mockState.playNext).toHaveBeenCalledTimes(1);
	});

	it("clears load errors when the audio element can play", () => {
		setQueue();

		render(<MusicPlayerHost />);

		const audio = document.querySelector("audio");
		if (!audio) {
			throw new Error("audio element not found");
		}

		fireEvent.canPlay(audio);

		expect(mockState.setError).toHaveBeenCalledWith(null);
	});

	it("resets and replays the same track when repeat one ends", () => {
		setQueue();
		mockState.state.playbackMode = "repeat_one";

		render(<MusicPlayerHost />);

		const audio = document.querySelector("audio");
		if (!audio) {
			throw new Error("audio element not found");
		}
		Object.defineProperty(audio, "currentTime", {
			configurable: true,
			writable: true,
			value: 42,
		});

		fireEvent.ended(audio);

		expect(audio.currentTime).toBe(0);
		expect(mockState.requestPlayback).toHaveBeenCalledTimes(1);
		expect(mockState.playNext).not.toHaveBeenCalled();
	});

	it("marks playback as stopped when the media element rejects play", async () => {
		const warnSpy = vi.spyOn(console, "warn").mockImplementation(() => {});
		vi.mocked(HTMLMediaElement.prototype.play).mockRejectedValueOnce(
			new Error("autoplay blocked"),
		);
		setQueue();
		mockState.state.playRequested = true;
		mockState.state.playRequestVersion = 1;

		render(<MusicPlayerHost />);

		await waitFor(() => {
			expect(mockState.setPlaybackRequested).toHaveBeenCalledWith(false);
		});
		expect(mockState.setPlaying).toHaveBeenCalledWith(false);
		expect(warnSpy).toHaveBeenCalledWith(
			"[AsterDrive]",
			"music playback start failed",
			"track-one.mp3",
			expect.any(Error),
		);
	});

	it("exposes track metadata and playback state to the system media session", () => {
		const { mediaSession } = installMockMediaSession();
		mockState.state.activeTrackId = "track-1";
		mockState.state.isPlaying = true;
		mockState.state.queue = [
			{
				id: "track-1",
				metadata: {
					album: "Album One",
					artist: "Fallback Artist",
					artists: ["Artist One", "Artist Two"],
					artworkUrl: "data:image/png;base64,cover",
					title: "Track One",
				},
				mimeType: "audio/mpeg",
				name: "track-one.mp3",
				path: "/files/7/download",
			},
		];

		render(<MusicPlayerHost />);

		expect(mediaSession.metadata).toMatchObject({
			album: "Album One",
			artist: "Artist One, Artist Two",
			artwork: [
				{
					src: "data:image/png;base64,cover",
					type: "image/png",
				},
			],
			title: "Track One",
		});
		expect(mediaSession.playbackState).toBe("playing");
	});

	it("uses the authenticated backend thumbnail blob as system media artwork when available", () => {
		const { mediaSession } = installMockMediaSession();
		mockState.useBlobUrl.mockReturnValue({
			blob: new Blob(["cover"], { type: "image/webp" }),
			blobUrl: "blob:backend-cover",
			error: false,
			loading: false,
			retry: vi.fn(),
		});
		setQueue();
		const { firstTrack, secondTrack } = getQueuedTracks();
		mockState.state.queue = [
			{
				...firstTrack,
				metadata: {
					...firstTrack.metadata,
					artworkUrl: "data:image/png;base64,fallback-cover",
				},
			},
			secondTrack,
		];

		render(<MusicPlayerHost />);

		expect(mockState.useBlobUrl).toHaveBeenCalledWith("/files/7/thumbnail", {
			lane: "thumbnail",
		});
		expect(mediaSession.metadata).toMatchObject({
			artwork: [
				{
					src: "blob:backend-cover",
				},
			],
			title: "Track One",
		});
	});

	it("falls back to parsed artwork for system media metadata while thumbnail support loads", () => {
		const { mediaSession } = installMockMediaSession();
		mockState.thumbnailSupportStore.config = null;
		mockState.thumbnailSupportStore.isLoaded = false;
		setQueue();
		const { firstTrack, secondTrack } = getQueuedTracks();
		mockState.state.queue = [
			{
				...firstTrack,
				metadata: {
					...firstTrack.metadata,
					artworkUrl: "data:image/png;base64,fallback-cover",
				},
			},
			secondTrack,
		];

		render(<MusicPlayerHost />);

		expect(mockState.thumbnailSupportStore.load).toHaveBeenCalledTimes(1);
		expect(mockState.useBlobUrl).toHaveBeenCalledWith(null, {
			lane: "thumbnail",
		});
		expect(mediaSession.metadata).toMatchObject({
			artwork: [
				{
					src: "data:image/png;base64,fallback-cover",
					type: "image/png",
				},
			],
			title: "Track One",
		});
	});

	it("wires system media controls to player actions", () => {
		const { fireAction, mediaSession } = installMockMediaSession();
		setQueue();

		render(<MusicPlayerHost />);

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
		vi.mocked(HTMLMediaElement.prototype.pause).mockClear();

		fireAction("play");
		fireAction("pause");
		fireAction("previoustrack");
		fireAction("nexttrack");
		fireAction("seekbackward", { seekOffset: 5 });
		fireAction("seekforward", { seekOffset: 10 });
		fireAction("seekto", { fastSeek: true, seekTime: 90 });
		fireAction("stop");

		expect(mockState.requestPlayback).toHaveBeenCalledTimes(1);
		expect(HTMLMediaElement.prototype.pause).toHaveBeenCalledTimes(2);
		expect(mockState.setPlaybackRequested).toHaveBeenCalledWith(false);
		expect(mockState.playPrevious).toHaveBeenCalledTimes(1);
		expect(mockState.playNext).toHaveBeenCalledTimes(1);
		expect(HTMLMediaElement.prototype.fastSeek).toHaveBeenCalledWith(90);
		expect(audio.currentTime).toBe(0);
		expect(mediaSession.setPositionState).toHaveBeenLastCalledWith({
			duration: 120,
			playbackRate: 1,
			position: 0,
		});
	});

	it("updates system media position from audio progress", () => {
		const { mediaSession } = installMockMediaSession();
		setQueue();

		render(<MusicPlayerHost />);

		const audio = document.querySelector("audio");
		if (!audio) {
			throw new Error("audio element not found");
		}
		Object.defineProperty(audio, "duration", {
			configurable: true,
			value: 180,
		});
		Object.defineProperty(audio, "currentTime", {
			configurable: true,
			writable: true,
			value: 45,
		});
		Object.defineProperty(audio, "playbackRate", {
			configurable: true,
			value: 1.25,
		});

		fireEvent.loadedMetadata(audio);
		fireEvent.timeUpdate(audio);

		expect(mediaSession.setPositionState).toHaveBeenLastCalledWith({
			duration: 180,
			playbackRate: 1.25,
			position: 45,
		});
	});

	it("updates the seek control from audio metadata and lets users seek", () => {
		setQueue();
		mockState.state.isPanelOpen = true;

		render(<MusicPlayerHost />);

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

		const seek = screen.getByRole("slider", { name: "music_player_seek" });
		expect(seek).toHaveValue("25");

		fireEvent.change(seek, { target: { value: "50" } });

		expect(audio.currentTime).toBe(60);
		expect(seek).toHaveValue("50");
	});

	it("renders buffered audio progress behind the active seek progress", () => {
		setQueue();
		mockState.state.isPanelOpen = true;

		render(<MusicPlayerHost />);

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
		Object.defineProperty(audio, "buffered", {
			configurable: true,
			value: {
				start: vi.fn(() => 0),
				end: vi.fn(() => 90),
				length: 1,
			},
		});

		fireEvent.loadedMetadata(audio);
		fireEvent.timeUpdate(audio);
		fireEvent.progress(audio);

		const seek = screen.getByRole("slider", { name: "music_player_seek" });
		const style = seek.getAttribute("style") ?? "";
		expect(seek).toHaveValue("25");
		expect(style).toContain("var(--color-primary) 25%");
		expect(style).toContain("var(--color-muted)) 75%");
	});

	it("updates duration and buffered progress from durationchange events", () => {
		setQueue();
		mockState.state.isPanelOpen = true;

		render(<MusicPlayerHost />);

		const audio = document.querySelector("audio");
		if (!audio) {
			throw new Error("audio element not found");
		}
		Object.defineProperty(audio, "duration", {
			configurable: true,
			value: 90,
		});
		Object.defineProperty(audio, "currentTime", {
			configurable: true,
			writable: true,
			value: 9,
		});
		Object.defineProperty(audio, "buffered", {
			configurable: true,
			value: {
				start: vi.fn(() => 0),
				end: vi.fn(() => 45),
				length: 1,
			},
		});

		fireEvent.durationChange(audio);
		fireEvent.timeUpdate(audio);

		const seek = screen.getByRole("slider", { name: "music_player_seek" });
		const style = seek.getAttribute("style") ?? "";
		expect(screen.getAllByText("1:30").length).toBeGreaterThan(0);
		expect(seek).toHaveValue("10");
		expect(style).toContain("var(--color-muted)) 50%");
	});

	it("pauses while scrubbing and resumes when the pointer seek ends", () => {
		setQueue();
		mockState.state.isPanelOpen = true;
		mockState.state.isPlaying = true;
		mockState.state.playRequested = true;

		render(<MusicPlayerHost />);

		const audio = document.querySelector("audio");
		if (!audio) {
			throw new Error("audio element not found");
		}
		Object.defineProperty(audio, "duration", {
			configurable: true,
			value: 120,
		});
		fireEvent.loadedMetadata(audio);

		const seek = screen.getByRole("slider", { name: "music_player_seek" });
		fireEvent.pointerDown(seek);
		fireEvent.pointerUp(seek);

		expect(HTMLMediaElement.prototype.pause).toHaveBeenCalledTimes(1);
		expect(mockState.setPlaybackRequested).toHaveBeenCalledWith(false);
		expect(mockState.requestPlayback).toHaveBeenCalledTimes(1);
	});

	it("clamps volume changes and switches the volume icon at zero", () => {
		setQueue();
		mockState.state.isPanelOpen = true;

		render(<MusicPlayerHost />);

		const volume = screen.getByRole("slider", { name: "music_player_volume" });
		expect(volume).toHaveValue("85");

		fireEvent.change(volume, { target: { value: "-25" } });

		expect(volume).toHaveValue("0");

		fireEvent.change(volume, { target: { value: "125" } });

		expect(volume).toHaveValue("100");
	});

	it("uses the buffered range nearest the playhead instead of later ranges", () => {
		setQueue();
		mockState.state.isPanelOpen = true;

		render(<MusicPlayerHost />);

		const audio = document.querySelector("audio");
		if (!audio) {
			throw new Error("audio element not found");
		}
		Object.defineProperty(audio, "duration", {
			configurable: true,
			value: 100,
		});
		Object.defineProperty(audio, "currentTime", {
			configurable: true,
			writable: true,
			value: 30,
		});
		Object.defineProperty(audio, "buffered", {
			configurable: true,
			value: {
				start: vi.fn((index: number) => (index === 0 ? 0 : 80)),
				end: vi.fn((index: number) => (index === 0 ? 25 : 100)),
				length: 2,
			},
		});

		fireEvent.loadedMetadata(audio);
		fireEvent.timeUpdate(audio);
		fireEvent.progress(audio);

		const seek = screen.getByRole("slider", { name: "music_player_seek" });
		const style = seek.getAttribute("style") ?? "";
		expect(seek).toHaveValue("30");
		expect(style).toContain("var(--color-primary) 30%");
		expect(style).toContain("var(--color-muted)) 30%");
		expect(style).not.toContain("var(--color-muted)) 100%");
	});

	it("falls back from pending backend metadata to browser parsing and then updates when backend metadata becomes ready", async () => {
		vi.useFakeTimers();
		const loadBackendMetadata = vi
			.fn()
			.mockRejectedValueOnce(new ApiPendingError("processing", 1))
			.mockResolvedValueOnce({
				artist: "Backend Artist",
				title: "Backend Title",
			});
		mockState.parseMusicMetadataFromSource.mockResolvedValueOnce({
			artist: "Parsed Artist",
			title: "Parsed Title",
		});
		mockState.state.activeTrackId = "track-1";
		mockState.state.queue = [
			{
				id: "track-1",
				loadBackendMetadata,
				metadata: { title: "Fallback Title" },
				mimeType: "audio/mpeg",
				name: "track.mp3",
				path: "/files/7/download",
			},
		];

		render(<MusicPlayerHost />);

		await flushAsyncEffects();

		expect(mockState.parseMusicMetadataFromSource).toHaveBeenCalledWith(
			expect.objectContaining({
				fallbackMetadata: { title: "Fallback Title" },
				source: "/api/v1/files/7/download",
			}),
		);
		expect(mockState.updateTrackMetadata).toHaveBeenCalledWith("track-1", {
			artist: "Parsed Artist",
			title: "Parsed Title",
		});

		await act(async () => {
			await vi.advanceTimersByTimeAsync(1_000);
		});

		expect(loadBackendMetadata).toHaveBeenCalledTimes(2);
		await act(async () => {
			await Promise.resolve();
		});
		expect(mockState.updateTrackMetadata).toHaveBeenCalledWith("track-1", {
			artist: "Backend Artist",
			title: "Backend Title",
		});
	});

	it("does not show stale buffered progress ahead of the playhead", () => {
		setQueue();
		mockState.state.isPanelOpen = true;

		render(<MusicPlayerHost />);

		const audio = document.querySelector("audio");
		if (!audio) {
			throw new Error("audio element not found");
		}
		Object.defineProperty(audio, "duration", {
			configurable: true,
			value: 100,
		});
		Object.defineProperty(audio, "currentTime", {
			configurable: true,
			writable: true,
			value: 30,
		});
		Object.defineProperty(audio, "buffered", {
			configurable: true,
			value: {
				start: vi.fn(() => 0),
				end: vi.fn(() => 20),
				length: 1,
			},
		});

		fireEvent.loadedMetadata(audio);
		fireEvent.timeUpdate(audio);
		fireEvent.progress(audio);

		const seek = screen.getByRole("slider", { name: "music_player_seek" });
		const style = seek.getAttribute("style") ?? "";
		expect(seek).toHaveValue("30");
		expect(style).toContain("var(--color-primary) 30%");
		expect(style).toContain("var(--color-muted)) 30%");
		expect(style).not.toContain("var(--color-muted)) 20%");
	});

	it("falls back to the start of the matching buffered range when current time is invalid", () => {
		setQueue();
		mockState.state.isPanelOpen = true;

		render(<MusicPlayerHost />);

		const audio = document.querySelector("audio");
		if (!audio) {
			throw new Error("audio element not found");
		}
		Object.defineProperty(audio, "duration", {
			configurable: true,
			value: 100,
		});
		Object.defineProperty(audio, "currentTime", {
			configurable: true,
			writable: true,
			value: Number.NaN,
		});
		Object.defineProperty(audio, "buffered", {
			configurable: true,
			value: {
				start: vi.fn(() => 0),
				end: vi.fn(() => 40),
				length: 1,
			},
		});

		fireEvent.loadedMetadata(audio);
		fireEvent.timeUpdate(audio);
		fireEvent.progress(audio);

		const seek = screen.getByRole("slider", { name: "music_player_seek" });
		const style = seek.getAttribute("style") ?? "";
		expect(seek).toHaveValue("0");
		expect(style).toContain("var(--color-primary) 0%");
		expect(style).toContain("var(--color-muted)) 40%");
	});

	it("renders zero buffered progress when audio has no buffered ranges", () => {
		setQueue();
		mockState.state.isPanelOpen = true;

		render(<MusicPlayerHost />);

		const audio = document.querySelector("audio");
		if (!audio) {
			throw new Error("audio element not found");
		}
		Object.defineProperty(audio, "duration", {
			configurable: true,
			value: 100,
		});
		Object.defineProperty(audio, "currentTime", {
			configurable: true,
			writable: true,
			value: 0,
		});
		Object.defineProperty(audio, "buffered", {
			configurable: true,
			value: {
				start: vi.fn(),
				end: vi.fn(),
				length: 0,
			},
		});

		fireEvent.loadedMetadata(audio);
		fireEvent.timeUpdate(audio);
		fireEvent.progress(audio);

		const seek = screen.getByRole("slider", { name: "music_player_seek" });
		const style = seek.getAttribute("style") ?? "";
		expect(seek).toHaveValue("0");
		expect(style).toContain("var(--color-primary) 0%");
		expect(style).toContain("var(--color-muted)) 0%");
	});

	it("clamps volume input values", () => {
		setQueue();
		mockState.state.isPanelOpen = true;

		render(<MusicPlayerHost />);

		const audio = document.querySelector("audio");
		if (!audio) {
			throw new Error("audio element not found");
		}
		const volume = screen.getByRole("slider", { name: "music_player_volume" });

		fireEvent.change(volume, { target: { value: "150" } });
		expect(audio.volume).toBe(1);

		fireEvent.change(volume, { target: { value: "-20" } });
		expect(audio.volume).toBe(0);
	});

	it("pauses while seeking and resumes only when it was previously playing", () => {
		setQueue();
		mockState.state.isPanelOpen = true;
		mockState.state.isPlaying = true;
		mockState.state.playRequested = true;

		render(<MusicPlayerHost />);

		const audio = document.querySelector("audio");
		if (!audio) {
			throw new Error("audio element not found");
		}
		Object.defineProperty(audio, "duration", {
			configurable: true,
			value: 120,
		});
		fireEvent.loadedMetadata(audio);

		const seek = screen.getByRole("slider", { name: "music_player_seek" });
		fireEvent.pointerDown(seek);
		fireEvent.change(seek, { target: { value: "50" } });
		fireEvent.pointerUp(seek);

		expect(HTMLMediaElement.prototype.pause).toHaveBeenCalledTimes(1);
		expect(mockState.setPlaybackRequested).toHaveBeenCalledWith(false);
		expect(mockState.requestPlayback).toHaveBeenCalledTimes(1);
		expect(audio.currentTime).toBe(60);
	});

	it("parses metadata once per track id so metadata updates do not re-fetch the same range", async () => {
		setQueue();
		mockState.state.isPanelOpen = true;

		const { rerender } = render(<MusicPlayerHost />);

		await flushAsyncEffects();

		const [firstTrack, secondTrack] = mockState.state.queue;
		if (!firstTrack || !secondTrack) {
			throw new Error("expected queued test tracks");
		}
		mockState.state.queue = [
			{
				...firstTrack,
				metadata: {
					artist: "Parsed Artist",
					artworkUrl: "data:image/jpeg;base64,cover",
					title: "Parsed Title",
				},
			},
			secondTrack,
		];
		rerender(<MusicPlayerHost />);

		await flushAsyncEffects();

		expect(mockState.parseMusicMetadataFromSource).toHaveBeenCalledTimes(1);
		expect(mockState.updateTrackMetadata).toHaveBeenCalledWith("track-1", {
			artist: "Parsed Artist",
			title: "Parsed Title",
		});
	});

	it("shows frontend fallback parsed metadata in details when backend metadata is unavailable", async () => {
		const loadBackendMetadata = vi
			.fn()
			.mockRejectedValueOnce(new Error("backend unavailable"));
		mockState.parseMusicMetadataFromSource.mockResolvedValueOnce({
			album: "Parsed Album",
			artist: "Parsed Artist",
			artists: ["Parsed Artist"],
			title: "Parsed Title",
		});
		setQueue();
		mockState.state.isPanelOpen = true;
		const { firstTrack, secondTrack } = getQueuedTracks();
		mockState.state.queue = [
			{
				...firstTrack,
				loadBackendMetadata,
			},
			secondTrack,
		];

		const { rerender } = render(<MusicPlayerHost />);

		await flushAsyncEffects();
		expect(loadBackendMetadata).toHaveBeenCalledTimes(1);
		expect(mockState.updateTrackMetadata).toHaveBeenCalledWith("track-1", {
			album: "Parsed Album",
			artist: "Parsed Artist",
			artists: ["Parsed Artist"],
			title: "Parsed Title",
		});

		rerender(<MusicPlayerHost />);
		fireEvent.click(
			screen.getByRole("button", { name: /music_player_details/i }),
		);

		expect(screen.getByText("music_player_title_label")).toBeInTheDocument();
		expect(screen.getAllByText("Parsed Title").length).toBeGreaterThan(0);
		expect(screen.getByText("music_player_artist_label")).toBeInTheDocument();
		expect(screen.getAllByText("Parsed Artist").length).toBeGreaterThan(0);
		expect(screen.getByText("music_player_album_label")).toBeInTheDocument();
		expect(screen.getByText("Parsed Album")).toBeInTheDocument();
	});

	it("logs metadata parse failures without surfacing player errors", async () => {
		const debugSpy = vi.spyOn(console, "debug").mockImplementation(() => {});
		const parseError = new Error("metadata failed");
		mockState.parseMusicMetadataFromSource.mockRejectedValueOnce(parseError);
		setQueue();

		render(<MusicPlayerHost />);

		await waitFor(() => {
			expect(debugSpy).toHaveBeenCalledWith(
				"[AsterDrive]",
				"music metadata parse failed",
				"track-one.mp3",
				parseError,
			);
		});
		expect(mockState.updateTrackMetadata).not.toHaveBeenCalled();
	});

	it("retries pending backend metadata using Retry-After while keeping fallback metadata", async () => {
		vi.useFakeTimers();
		const debugSpy = vi.spyOn(console, "debug").mockImplementation(() => {});
		const loadBackendMetadata = vi
			.fn()
			.mockRejectedValueOnce(new ApiPendingError("pending", 3))
			.mockResolvedValueOnce({
				artist: "Backend Artist",
				title: "Backend Title",
			});
		setQueue();
		const { firstTrack, secondTrack } = getQueuedTracks();
		mockState.state.queue = [
			{
				...firstTrack,
				loadBackendMetadata,
			},
			secondTrack,
		];

		render(<MusicPlayerHost />);

		await flushAsyncEffects();
		expect(mockState.updateTrackMetadata).toHaveBeenCalledWith("track-1", {
			artist: "Parsed Artist",
			title: "Parsed Title",
		});
		expect(loadBackendMetadata).toHaveBeenCalledTimes(1);

		await act(async () => {
			vi.advanceTimersByTime(2_999);
		});
		expect(loadBackendMetadata).toHaveBeenCalledTimes(1);

		await act(async () => {
			vi.advanceTimersByTime(1);
		});
		await flushAsyncEffects();

		expect(mockState.updateTrackMetadata).toHaveBeenCalledWith("track-1", {
			artist: "Backend Artist",
			title: "Backend Title",
		});
		expect(loadBackendMetadata).toHaveBeenCalledTimes(2);
		expect(debugSpy).toHaveBeenCalledWith(
			"[AsterDrive]",
			"backend music metadata pending",
			"track-one.mp3",
			expect.any(ApiPendingError),
		);
	});

	it("does not apply retried backend metadata after switching tracks", async () => {
		vi.useFakeTimers();
		const loadBackendMetadata = vi
			.fn()
			.mockRejectedValueOnce(new ApiPendingError("pending", 1))
			.mockResolvedValueOnce({
				artist: "Late Artist",
				title: "Late Title",
			});
		setQueue();
		const { firstTrack, secondTrack } = getQueuedTracks();
		mockState.state.queue = [
			{
				...firstTrack,
				loadBackendMetadata,
			},
			secondTrack,
		];

		const { unmount } = render(<MusicPlayerHost />);

		await flushAsyncEffects();
		expect(mockState.updateTrackMetadata).toHaveBeenCalledWith("track-1", {
			artist: "Parsed Artist",
			title: "Parsed Title",
		});
		mockState.updateTrackMetadata.mockClear();
		mockState.state.activeTrackId = "track-2";
		unmount();

		await act(async () => {
			vi.advanceTimersByTime(1_000);
		});
		await flushAsyncEffects();

		expect(loadBackendMetadata).toHaveBeenCalledTimes(1);
		expect(mockState.updateTrackMetadata).not.toHaveBeenCalled();
	});

	it("moves to the next track when the current track ends", () => {
		setQueue();

		render(<MusicPlayerHost />);

		const audio = document.querySelector("audio");
		if (!audio) {
			throw new Error("audio element not found");
		}

		fireEvent.ended(audio);

		expect(mockState.playNext).toHaveBeenCalledTimes(1);
	});

	it("keeps system next control usable after a load error", () => {
		const { fireAction } = installMockMediaSession();
		setQueue();

		render(<MusicPlayerHost />);

		const audio = document.querySelector("audio");
		if (!audio) {
			throw new Error("audio element not found");
		}

		fireEvent.error(audio);
		fireAction("nexttrack");

		expect(mockState.playNext).toHaveBeenCalledTimes(1);
	});

	it("refreshes expiring stream sessions before the current link expires", async () => {
		vi.useFakeTimers();
		const refreshStreamLink = vi.fn(async () => ({
			expires_at: "2026-01-01T03:00:00Z",
			path: "/api/v1/s/share-token/stream/session-2/track.mp3",
		}));
		vi.setSystemTime(new Date("2026-01-01T00:00:00Z"));
		mockState.state.activeTrackId = "track-1";
		mockState.state.queue = [
			{
				expiresAt: "2026-01-01T00:03:00Z",
				id: "track-1",
				mimeType: "audio/mpeg",
				name: "track.mp3",
				path: "/api/v1/s/share-token/stream/session-1/track.mp3",
				refreshStreamLink,
			},
		];

		render(<MusicPlayerHost />);

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

	it("logs stream session refresh failures", async () => {
		vi.useFakeTimers();
		const refreshError = new Error("refresh failed");
		const warnSpy = vi.spyOn(console, "warn").mockImplementation(() => {});
		const refreshStreamLink = vi.fn(async () => {
			throw refreshError;
		});
		vi.setSystemTime(new Date("2026-01-01T00:00:00Z"));
		mockState.state.activeTrackId = "track-1";
		mockState.state.queue = [
			{
				expiresAt: "2026-01-01T00:03:00Z",
				id: "track-1",
				mimeType: "audio/mpeg",
				name: "track.mp3",
				path: "/api/v1/s/share-token/stream/session-1/track.mp3",
				refreshStreamLink,
			},
		];

		render(<MusicPlayerHost />);

		await act(async () => {
			vi.advanceTimersByTime(60_000);
			await Promise.resolve();
		});

		expect(refreshStreamLink).toHaveBeenCalledTimes(1);
		expect(mockState.updateTrackSource).not.toHaveBeenCalled();
		expect(warnSpy).toHaveBeenCalledWith(
			"[AsterDrive]",
			"music stream session refresh failed",
			"track.mp3",
			refreshError,
		);
		warnSpy.mockRestore();
	});
});
