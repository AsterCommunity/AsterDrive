import type { ChangeEvent, CSSProperties, ReactNode } from "react";
import {
	useCallback,
	useEffect,
	useId,
	useLayoutEffect,
	useMemo,
	useRef,
	useState,
} from "react";
import { useTranslation } from "react-i18next";
import { AnimatedCollapsible } from "@/components/common/AnimatedCollapsible";
import { Badge } from "@/components/ui/badge";
import { Button } from "@/components/ui/button";
import { Icon } from "@/components/ui/icon";
import {
	Tooltip,
	TooltipContent,
	TooltipProvider,
	TooltipTrigger,
} from "@/components/ui/tooltip";
import { resolveApiResourceUrl } from "@/lib/apiUrl";
import { formatBytes } from "@/lib/format";
import { logger } from "@/lib/logger";
import { parseMusicMetadataFromSource } from "@/lib/musicPlayer";
import { cn } from "@/lib/utils";
import {
	type MusicPlaybackMode,
	type MusicPlayerTrack,
	useMusicPlayerStore,
} from "@/stores/musicPlayerStore";

const STREAM_REFRESH_LEAD_MS = 2 * 60 * 1000;
const STREAM_REFRESH_MIN_DELAY_MS = 10 * 1000;
const MEDIA_SESSION_SEEK_OFFSET_SECONDS = 10;
const PLAYBACK_ERROR_SKIP_DELAY_MS = 2_000;
const MEDIA_SESSION_ACTIONS: MediaSessionAction[] = [
	"play",
	"pause",
	"previoustrack",
	"nexttrack",
	"seekbackward",
	"seekforward",
	"seekto",
	"stop",
];
const MUSIC_PLAYER_INTERNAL_SELECTOR =
	"[data-music-player-surface],[data-music-player-trigger]";
const BUFFERED_RANGE_CURRENT_TIME_TOLERANCE_SECONDS = 0.5;

function formatPlaybackTime(seconds: number) {
	if (!Number.isFinite(seconds) || seconds < 0) {
		return "0:00";
	}

	const totalSeconds = Math.floor(seconds);
	const minutes = Math.floor(totalSeconds / 60);
	const remainingSeconds = totalSeconds % 60;
	return `${minutes}:${remainingSeconds.toString().padStart(2, "0")}`;
}

function clampPercent(value: number) {
	return Math.min(100, Math.max(0, value));
}

function rangeFillStyle(value: number, bufferedValue = value): CSSProperties {
	const percent = clampPercent(value);
	const bufferedPercent = Math.max(percent, clampPercent(bufferedValue));
	return {
		background: `linear-gradient(to right, var(--color-primary) 0%, var(--color-primary) ${percent}%, color-mix(in oklab, var(--color-primary) 35%, var(--color-muted)) ${percent}%, color-mix(in oklab, var(--color-primary) 35%, var(--color-muted)) ${bufferedPercent}%, var(--color-muted) ${bufferedPercent}%, var(--color-muted) 100%)`,
	};
}

function sessionRefreshDelay(expiresAt?: string) {
	if (!expiresAt) return null;

	const expiresAtMs = new Date(expiresAt).getTime();
	if (!Number.isFinite(expiresAtMs)) return null;

	return Math.max(
		STREAM_REFRESH_MIN_DELAY_MS,
		expiresAtMs - Date.now() - STREAM_REFRESH_LEAD_MS,
	);
}

function displayTitle(track: MusicPlayerTrack) {
	return track.metadata?.title || track.name;
}

function displayArtist(track: MusicPlayerTrack) {
	if (track.metadata?.artists && track.metadata.artists.length > 0) {
		return track.metadata.artists.join(", ");
	}
	return track.metadata?.artist || null;
}

function getMusicMediaSession() {
	if (typeof navigator === "undefined" || !("mediaSession" in navigator)) {
		return null;
	}
	return navigator.mediaSession;
}

function mediaArtworkFromUrl(src: string): MediaImage {
	const mimeMatch = src.match(/^data:([^;,]+)/);
	const type = mimeMatch?.[1];
	return type ? { src, type } : { src };
}

function createMediaSessionMetadata(track: MusicPlayerTrack) {
	if (typeof MediaMetadata === "undefined") {
		return null;
	}

	const artworkUrl = track.metadata?.artworkUrl;
	const metadata: MediaMetadataInit = {
		album: track.metadata?.album ?? "",
		artist: displayArtist(track) ?? "",
		title: displayTitle(track),
	};

	if (artworkUrl) {
		metadata.artwork = [mediaArtworkFromUrl(artworkUrl)];
	}

	return new MediaMetadata(metadata);
}

function setMediaSessionActionHandler(
	mediaSession: MediaSession,
	action: MediaSessionAction,
	handler: MediaSessionActionHandler | null,
) {
	try {
		mediaSession.setActionHandler(action, handler);
	} catch (error) {
		logger.debug("media session action handler ignored", action, error);
	}
}

function clearMediaSessionPosition(mediaSession: MediaSession) {
	if (typeof mediaSession.setPositionState !== "function") return;

	try {
		mediaSession.setPositionState();
	} catch (error) {
		logger.debug("media session position clear failed", error);
	}
}

function updateMediaSessionPosition(
	mediaSession: MediaSession,
	audio: HTMLAudioElement | null,
	duration: number,
	currentTime: number,
) {
	if (typeof mediaSession.setPositionState !== "function") return;

	if (!Number.isFinite(duration) || duration <= 0) {
		clearMediaSessionPosition(mediaSession);
		return;
	}

	const position = Math.min(
		duration,
		Math.max(0, Number.isFinite(currentTime) ? currentTime : 0),
	);
	const playbackRate =
		audio && Number.isFinite(audio.playbackRate) && audio.playbackRate > 0
			? audio.playbackRate
			: 1;

	try {
		mediaSession.setPositionState({
			duration,
			playbackRate,
			position,
		});
	} catch (error) {
		logger.debug("media session position update failed", error);
	}
}

function setAudioCurrentTime(
	audio: HTMLAudioElement,
	currentTime: number,
	fastSeek: boolean,
) {
	if (fastSeek && typeof audio.fastSeek === "function") {
		try {
			audio.fastSeek(currentTime);
			return;
		} catch (error) {
			logger.debug("audio fast seek failed", error);
		}
	}

	audio.currentTime = currentTime;
}

function bufferedProgressFromAudio(
	audio: HTMLAudioElement | null,
	duration: number,
) {
	if (!audio || !Number.isFinite(duration) || duration <= 0) return 0;

	const currentTime = Number.isFinite(audio.currentTime)
		? audio.currentTime
		: 0;
	let bufferedEnd = currentTime;
	const { buffered } = audio;
	for (let index = 0; index < buffered.length; index += 1) {
		const rangeStart = buffered.start(index);
		const rangeEnd = buffered.end(index);
		if (
			Number.isFinite(rangeStart) &&
			Number.isFinite(rangeEnd) &&
			rangeStart <=
				currentTime + BUFFERED_RANGE_CURRENT_TIME_TOLERANCE_SECONDS &&
			rangeEnd > bufferedEnd
		) {
			bufferedEnd = rangeEnd;
		}
	}

	return clampPercent((bufferedEnd / duration) * 100);
}

function isMusicPlayerInteractionTarget(
	event: MouseEvent,
	panel: HTMLElement | null,
) {
	const path = event.composedPath();
	if (
		path.some((entry) => {
			if (entry === panel) return true;
			return (
				entry instanceof Element &&
				Boolean(entry.closest(MUSIC_PLAYER_INTERNAL_SELECTOR))
			);
		})
	) {
		return true;
	}

	const target = event.target;
	if (!(target instanceof Node)) return false;
	if (panel?.contains(target)) return true;

	const targetElement =
		target instanceof Element ? target : target.parentElement;
	return Boolean(targetElement?.closest(MUSIC_PLAYER_INTERNAL_SELECTOR));
}

const MUSIC_TEXT_MARQUEE_KEYFRAMES = `
@keyframes music-player-text-marquee {
	0% {
		transform: translateX(0);
	}
	12% {
		transform: translateX(0);
	}
	82% {
		transform: translateX(var(--music-text-scroll-distance));
	}
	100% {
		transform: translateX(var(--music-text-scroll-distance));
	}
}
`;
const MUSIC_TEXT_MARQUEE_COPY_GAP_PX = 24;

function useAutoScrollState(text: string, enabled: boolean) {
	const viewportRef = useRef<HTMLDivElement | null>(null);
	const trackRef = useRef<HTMLDivElement | HTMLSpanElement | null>(null);
	const [isOverflowing, setIsOverflowing] = useState(false);
	const [scrollDistance, setScrollDistance] = useState(0);

	useLayoutEffect(() => {
		// `text` is read here so the effect reruns and remeasures when the label changes.
		void text;
		if (!enabled) {
			setIsOverflowing(false);
			setScrollDistance(0);
			return;
		}

		const measure = () => {
			const viewport = viewportRef.current;
			const track = trackRef.current;
			if (!viewport || !track) return;
			const overflowDistance = Math.max(
				0,
				track.scrollWidth - viewport.clientWidth,
			);
			setScrollDistance(track.scrollWidth + MUSIC_TEXT_MARQUEE_COPY_GAP_PX);
			setIsOverflowing(overflowDistance > 1);
		};

		measure();

		const viewport = viewportRef.current;
		const ro =
			typeof ResizeObserver === "undefined"
				? null
				: new ResizeObserver(() => {
						measure();
					});

		if (viewport) {
			ro?.observe(viewport);
		}
		if (trackRef.current) {
			ro?.observe(trackRef.current);
		}

		const raf = window.requestAnimationFrame(measure);
		window.addEventListener("resize", measure);

		return () => {
			window.cancelAnimationFrame(raf);
			window.removeEventListener("resize", measure);
			ro?.disconnect();
		};
	}, [enabled, text]);

	return { isOverflowing, scrollDistance, trackRef, viewportRef };
}

function AutoScrollText({
	active,
	children,
	className,
}: {
	active: boolean;
	children: string;
	className?: string;
}) {
	const { isOverflowing, scrollDistance, trackRef, viewportRef } =
		useAutoScrollState(children, active);
	const shouldMarquee = active && isOverflowing;
	const animationDuration = Math.min(28, Math.max(12, 8 + scrollDistance / 36));

	return (
		<div
			ref={viewportRef}
			className={cn("min-w-0 overflow-hidden", "select-text", className)}
			data-marquee-active={String(shouldMarquee)}
		>
			{shouldMarquee ? (
				<span
					className={cn(
						"flex w-max max-w-none gap-6 whitespace-nowrap will-change-transform hover:[animation-play-state:paused] motion-reduce:[animation:none]",
					)}
					style={
						{
							animation: `music-player-text-marquee ${animationDuration}s linear infinite`,
							"--music-text-scroll-distance": `-${scrollDistance}px`,
						} as CSSProperties
					}
				>
					<span
						ref={(node) => {
							trackRef.current = node;
						}}
						className="shrink-0"
					>
						{children}
					</span>
					<span className="shrink-0" aria-hidden="true">
						{children}
					</span>
				</span>
			) : (
				<span
					ref={(node) => {
						trackRef.current = node;
					}}
					className="block min-w-0 truncate whitespace-nowrap"
				>
					{children}
				</span>
			)}
		</div>
	);
}

function playbackModeIcon(mode: MusicPlaybackMode) {
	if (mode === "shuffle") return "Shuffle";
	if (mode === "repeat_one") return "RepeatOnce";
	return "Repeat";
}

function nextPlaybackMode(mode: MusicPlaybackMode): MusicPlaybackMode {
	if (mode === "repeat_queue") return "repeat_one";
	if (mode === "repeat_one") return "shuffle";
	return "repeat_queue";
}

function MusicArtwork({
	className,
	track,
}: {
	className?: string;
	track: MusicPlayerTrack | null;
}) {
	if (track?.metadata?.artworkUrl) {
		return (
			<img
				src={track.metadata.artworkUrl}
				alt=""
				className={cn("object-cover", className)}
			/>
		);
	}

	return (
		<div
			className={cn(
				"flex items-center justify-center overflow-hidden rounded-lg border border-border/55 bg-[linear-gradient(135deg,var(--color-muted),var(--color-background))] text-primary",
				className,
			)}
		>
			<Icon name="VinylRecord" className="h-1/2 w-1/2 opacity-80" />
		</div>
	);
}

function PlayerIconButton({
	active = false,
	children,
	label,
	onClick,
}: {
	active?: boolean;
	children: ReactNode;
	label: string;
	onClick: () => void;
}) {
	return (
		<Tooltip>
			<TooltipTrigger
				render={
					<Button
						type="button"
						variant={active ? "secondary" : "ghost"}
						size="icon-sm"
						onClick={onClick}
						aria-label={label}
					/>
				}
			>
				{children}
			</TooltipTrigger>
			<TooltipContent>{label}</TooltipContent>
		</Tooltip>
	);
}

export function MusicPlayerHost() {
	const { t } = useTranslation("files");
	const audioRef = useRef<HTMLAudioElement | null>(null);
	const panelRef = useRef<HTMLDivElement | null>(null);
	const currentTimeRef = useRef(0);
	const durationRef = useRef(0);
	const errorSkipTimerRef = useRef<number | null>(null);
	const isSeekingRef = useRef(false);
	const parsedMetadataTrackIdsRef = useRef(new Set<string>());
	const wasPlayingBeforeSeekRef = useRef(false);
	const latestTrackIdRef = useRef<string | null>(null);
	const activeQueueItemRef = useRef<HTMLButtonElement | null>(null);
	const queueUserActivationTrackIdRef = useRef<string | null>(null);
	const queuePanelId = useId();
	const detailsPanelId = useId();
	const [bufferedProgress, setBufferedProgress] = useState(0);
	const [currentTime, setCurrentTime] = useState(0);
	const [duration, setDuration] = useState(0);
	const [detailsOpen, setDetailsOpen] = useState(false);
	const [queueOpen, setQueueOpen] = useState(false);
	const [volume, setVolume] = useState(0.85);
	const activeTrackId = useMusicPlayerStore((state) => state.activeTrackId);
	const error = useMusicPlayerStore((state) => state.error);
	const isPanelOpen = useMusicPlayerStore((state) => state.isPanelOpen);
	const isPlaying = useMusicPlayerStore((state) => state.isPlaying);
	const playRequested = useMusicPlayerStore((state) => state.playRequested);
	const playRequestVersion = useMusicPlayerStore(
		(state) => state.playRequestVersion,
	);
	const playbackMode = useMusicPlayerStore((state) => state.playbackMode);
	const queue = useMusicPlayerStore((state) => state.queue);
	const closePanel = useMusicPlayerStore((state) => state.closePanel);
	const clear = useMusicPlayerStore((state) => state.clear);
	const playNext = useMusicPlayerStore((state) => state.playNext);
	const playPrevious = useMusicPlayerStore((state) => state.playPrevious);
	const playTracks = useMusicPlayerStore((state) => state.playTracks);
	const requestPlayback = useMusicPlayerStore((state) => state.requestPlayback);
	const setError = useMusicPlayerStore((state) => state.setError);
	const setPlaybackMode = useMusicPlayerStore((state) => state.setPlaybackMode);
	const setPlaying = useMusicPlayerStore((state) => state.setPlaying);
	const setPlaybackRequested = useMusicPlayerStore(
		(state) => state.setPlaybackRequested,
	);
	const updateTrackMetadata = useMusicPlayerStore(
		(state) => state.updateTrackMetadata,
	);
	const updateTrackSource = useMusicPlayerStore(
		(state) => state.updateTrackSource,
	);
	const track = useMemo(
		() => queue.find((candidate) => candidate.id === activeTrackId) ?? null,
		[activeTrackId, queue],
	);
	useEffect(() => {
		latestTrackIdRef.current = track?.id ?? null;
	}, [track?.id]);
	const source = useMemo(
		() => (track ? resolveApiResourceUrl(track.path) : null),
		[track],
	);
	const trackKey = track ? `${track.id}:${track.path}` : null;
	const progress =
		duration > 0 && Number.isFinite(duration)
			? Math.min(100, Math.max(0, (currentTime / duration) * 100))
			: 0;
	const volumePercent = Math.round(volume * 100);
	const modeLabel = t(`music_player_mode_${playbackMode}`);

	useEffect(() => {
		currentTimeRef.current = currentTime;
		durationRef.current = duration;
	}, [currentTime, duration]);

	const clearPendingErrorSkip = useCallback(() => {
		if (errorSkipTimerRef.current === null) return;
		window.clearTimeout(errorSkipTimerRef.current);
		errorSkipTimerRef.current = null;
	}, []);

	const syncBufferedProgress = useCallback((audio: HTMLAudioElement | null) => {
		setBufferedProgress(bufferedProgressFromAudio(audio, durationRef.current));
	}, []);

	const scheduleNextAfterPlaybackError = useCallback(
		(failedTrackId: string | null) => {
			clearPendingErrorSkip();
			if (!failedTrackId) return;

			errorSkipTimerRef.current = window.setTimeout(() => {
				errorSkipTimerRef.current = null;
				if (latestTrackIdRef.current !== failedTrackId) return;
				playNext();
			}, PLAYBACK_ERROR_SKIP_DELAY_MS);
		},
		[clearPendingErrorSkip, playNext],
	);

	const seekAudioTo = useCallback((nextTime: number, fastSeek = false) => {
		const audio = audioRef.current;
		const mediaDuration = durationRef.current;
		if (
			!audio ||
			!Number.isFinite(nextTime) ||
			!Number.isFinite(mediaDuration) ||
			mediaDuration <= 0
		) {
			return;
		}

		const clampedTime = Math.min(mediaDuration, Math.max(0, nextTime));
		setAudioCurrentTime(audio, clampedTime, fastSeek);
		currentTimeRef.current = clampedTime;
		setCurrentTime(clampedTime);

		const mediaSession = getMusicMediaSession();
		if (mediaSession) {
			updateMediaSessionPosition(
				mediaSession,
				audio,
				mediaDuration,
				clampedTime,
			);
		}
	}, []);

	const seekAudioBy = useCallback(
		(offset: number) => {
			const audio = audioRef.current;
			const baseTime =
				audio && Number.isFinite(audio.currentTime)
					? audio.currentTime
					: currentTimeRef.current;
			seekAudioTo(baseTime + offset);
		},
		[seekAudioTo],
	);

	const playPreviousTrack = useCallback(() => {
		clearPendingErrorSkip();
		playPrevious();
	}, [clearPendingErrorSkip, playPrevious]);

	const playNextTrack = useCallback(() => {
		clearPendingErrorSkip();
		playNext();
	}, [clearPendingErrorSkip, playNext]);

	useEffect(() => {
		if (!trackKey) return;
		currentTimeRef.current = 0;
		durationRef.current = 0;
		setBufferedProgress(0);
		setCurrentTime(0);
		setDuration(0);
		clearPendingErrorSkip();
		setError(null);
	}, [clearPendingErrorSkip, setError, trackKey]);

	useEffect(() => {
		return () => {
			clearPendingErrorSkip();
		};
	}, [clearPendingErrorSkip]);

	useEffect(() => {
		if (!isPanelOpen) return;

		const handleDocumentClick = (event: MouseEvent) => {
			if (isMusicPlayerInteractionTarget(event, panelRef.current)) return;
			closePanel();
		};

		document.addEventListener("click", handleDocumentClick);
		return () => {
			document.removeEventListener("click", handleDocumentClick);
		};
	}, [closePanel, isPanelOpen]);

	useEffect(() => {
		if (isPanelOpen) return;
		setDetailsOpen(false);
		setQueueOpen(false);
	}, [isPanelOpen]);

	useEffect(() => {
		if (!queueOpen || !activeTrackId) return;

		if (queueUserActivationTrackIdRef.current === activeTrackId) {
			queueUserActivationTrackIdRef.current = null;
			return;
		}
		queueUserActivationTrackIdRef.current = null;

		activeQueueItemRef.current?.scrollIntoView({
			block: "nearest",
			inline: "nearest",
		});
	}, [activeTrackId, queueOpen]);

	useEffect(() => {
		if (!track || !source || parsedMetadataTrackIdsRef.current.has(track.id)) {
			return;
		}
		parsedMetadataTrackIdsRef.current.add(track.id);

		const controller = new AbortController();
		const trackId = track.id;
		const fallbackMetadata = track.metadata;
		const mimeType = track.mimeType;
		const name = track.name;
		const size = track.size;
		void parseMusicMetadataFromSource({
			fallbackMetadata,
			mimeType,
			name,
			signal: controller.signal,
			size,
			source,
		})
			.then((metadata) => {
				updateTrackMetadata(trackId, metadata);
			})
			.catch((metadataError) => {
				if (controller.signal.aborted) return;
				logger.debug("music metadata parse failed", name, metadataError);
			});

		return () => {
			controller.abort();
		};
	}, [source, track, updateTrackMetadata]);

	useEffect(() => {
		const audio = audioRef.current;
		if (!audio) return;
		audio.volume = volume;
	}, [volume]);

	useEffect(() => {
		const mediaSession = getMusicMediaSession();
		if (!mediaSession) return;

		if (!track) {
			mediaSession.metadata = null;
			mediaSession.playbackState = "none";
			clearMediaSessionPosition(mediaSession);
			return;
		}

		mediaSession.metadata = createMediaSessionMetadata(track);
	}, [track]);

	useEffect(() => {
		const mediaSession = getMusicMediaSession();
		if (!mediaSession) return;

		mediaSession.playbackState = track
			? isPlaying
				? "playing"
				: "paused"
			: "none";
	}, [isPlaying, track]);

	useEffect(() => {
		const mediaSession = getMusicMediaSession();
		if (!mediaSession || !track) return;

		setMediaSessionActionHandler(mediaSession, "play", () => {
			requestPlayback();
		});
		setMediaSessionActionHandler(mediaSession, "pause", () => {
			audioRef.current?.pause();
			setPlaybackRequested(false);
		});
		setMediaSessionActionHandler(mediaSession, "previoustrack", () => {
			playPreviousTrack();
		});
		setMediaSessionActionHandler(mediaSession, "nexttrack", () => {
			playNextTrack();
		});
		setMediaSessionActionHandler(mediaSession, "seekbackward", (details) => {
			seekAudioBy(-(details.seekOffset ?? MEDIA_SESSION_SEEK_OFFSET_SECONDS));
		});
		setMediaSessionActionHandler(mediaSession, "seekforward", (details) => {
			seekAudioBy(details.seekOffset ?? MEDIA_SESSION_SEEK_OFFSET_SECONDS);
		});
		setMediaSessionActionHandler(mediaSession, "seekto", (details) => {
			if (typeof details.seekTime !== "number") return;
			seekAudioTo(details.seekTime, details.fastSeek ?? false);
		});
		setMediaSessionActionHandler(mediaSession, "stop", () => {
			audioRef.current?.pause();
			setPlaybackRequested(false);
			seekAudioTo(0);
		});

		return () => {
			for (const action of MEDIA_SESSION_ACTIONS) {
				setMediaSessionActionHandler(mediaSession, action, null);
			}
		};
	}, [
		playNextTrack,
		playPreviousTrack,
		requestPlayback,
		seekAudioBy,
		seekAudioTo,
		setPlaybackRequested,
		track,
	]);

	useEffect(() => {
		const mediaSession = getMusicMediaSession();
		if (!mediaSession || !track) return;

		updateMediaSessionPosition(
			mediaSession,
			audioRef.current,
			duration,
			currentTime,
		);
	}, [currentTime, duration, track]);

	useEffect(() => {
		if (!track?.refreshStreamLink) return;

		const delay = sessionRefreshDelay(track.expiresAt);
		if (delay === null) return;

		const timer = window.setTimeout(() => {
			const scheduledTrackId = track.id;
			if (latestTrackIdRef.current !== scheduledTrackId) {
				return;
			}
			track
				.refreshStreamLink?.()
				.then((link) => {
					if (latestTrackIdRef.current !== scheduledTrackId) {
						return;
					}
					updateTrackSource(track.id, link);
				})
				.catch((refreshError) => {
					logger.warn(
						"music stream session refresh failed",
						track.name,
						refreshError,
					);
				});
		}, delay);

		return () => window.clearTimeout(timer);
	}, [track, updateTrackSource]);

	useEffect(() => {
		const audio = audioRef.current;
		if (!audio || !source) return;
		void playRequestVersion;

		if (!playRequested) {
			audio.pause();
			return;
		}

		void audio.play().catch((playError) => {
			logger.warn("music playback start failed", track?.name, playError);
			setError(t("music_player_load_failed"));
			setPlaybackRequested(false);
			setPlaying(false);
			scheduleNextAfterPlaybackError(track?.id ?? null);
		});
	}, [
		playRequestVersion,
		playRequested,
		scheduleNextAfterPlaybackError,
		setError,
		setPlaybackRequested,
		setPlaying,
		source,
		t,
		track?.id,
		track?.name,
	]);

	if (!track || !source) {
		return null;
	}

	const togglePlayback = () => {
		if (isPlaying) {
			audioRef.current?.pause();
			setPlaybackRequested(false);
			return;
		}

		clearPendingErrorSkip();
		requestPlayback();
	};

	const handleSeek = (event: ChangeEvent<HTMLInputElement>) => {
		if (duration <= 0) return;

		const nextTime = (Number(event.currentTarget.value) / 100) * duration;
		seekAudioTo(nextTime);
	};

	const beginSeek = () => {
		if (isSeekingRef.current) return;
		isSeekingRef.current = true;
		wasPlayingBeforeSeekRef.current = isPlaying || playRequested;

		if (wasPlayingBeforeSeekRef.current) {
			audioRef.current?.pause();
			setPlaybackRequested(false);
		}
	};

	const endSeek = () => {
		if (!isSeekingRef.current) return;
		isSeekingRef.current = false;

		if (wasPlayingBeforeSeekRef.current) {
			requestPlayback();
		}
		wasPlayingBeforeSeekRef.current = false;
	};

	const handleVolumeChange = (event: ChangeEvent<HTMLInputElement>) => {
		const nextVolume = Number(event.currentTarget.value) / 100;
		if (!Number.isFinite(nextVolume)) return;
		setVolume(Math.min(1, Math.max(0, nextVolume)));
	};

	const activateQueueTrack = (trackId: string) => {
		clearPendingErrorSkip();
		queueUserActivationTrackIdRef.current = trackId;
		playTracks(queue, trackId);
	};

	return (
		<>
			<style>{MUSIC_TEXT_MARQUEE_KEYFRAMES}</style>

			{/* biome-ignore lint/a11y/useMediaCaption: user-uploaded media may not have captions available */}
			<audio
				ref={audioRef}
				src={source ?? undefined}
				preload="metadata"
				onCanPlay={() => {
					clearPendingErrorSkip();
					setError(null);
					syncBufferedProgress(audioRef.current);
				}}
				onDurationChange={(event) => {
					const nextDuration = event.currentTarget.duration || 0;
					durationRef.current = nextDuration;
					setDuration(nextDuration);
					setBufferedProgress(
						bufferedProgressFromAudio(event.currentTarget, nextDuration),
					);
				}}
				onEnded={() => {
					if (playbackMode === "repeat_one") {
						const audio = audioRef.current;
						if (audio) {
							audio.currentTime = 0;
						}
						requestPlayback();
						return;
					}
					playNextTrack();
				}}
				onError={() => {
					setError(t("music_player_load_failed"));
					setPlaybackRequested(false);
					setPlaying(false);
					scheduleNextAfterPlaybackError(track.id);
				}}
				onLoadedMetadata={(event) => {
					const nextDuration = event.currentTarget.duration || 0;
					durationRef.current = nextDuration;
					setDuration(nextDuration);
					setBufferedProgress(
						bufferedProgressFromAudio(event.currentTarget, nextDuration),
					);
				}}
				onPause={() => setPlaying(false)}
				onPlay={() => {
					clearPendingErrorSkip();
					setError(null);
					setPlaying(true);
					setPlaybackRequested(true);
				}}
				onProgress={(event) => {
					syncBufferedProgress(event.currentTarget);
				}}
				onTimeUpdate={(event) => {
					const nextTime = event.currentTarget.currentTime || 0;
					currentTimeRef.current = nextTime;
					setCurrentTime(nextTime);
					syncBufferedProgress(event.currentTarget);
				}}
			/>

			<div
				ref={panelRef}
				aria-hidden={!isPanelOpen}
				data-music-player-surface
				data-state={isPanelOpen ? "open" : "closed"}
				inert={isPanelOpen ? undefined : true}
				className={cn(
					"fixed top-[calc(var(--spacing)*16+0.5rem)] right-3 z-40 w-[calc(100vw-1.5rem)] max-w-[26rem] origin-top-right transition-[opacity,transform] duration-150 ease-out motion-reduce:transition-none sm:right-4",
					isPanelOpen
						? "translate-y-0 scale-100 opacity-100"
						: "pointer-events-none -translate-y-2 scale-[0.98] opacity-0",
				)}
			>
				<section
					aria-label={t("music_player_title")}
					data-theme-surface="overlay"
					className="max-h-[calc(100vh-4.5rem)] overflow-hidden rounded-lg border border-border/70 bg-popover/96 text-sm shadow-2xl shadow-black/12 ring-1 ring-foreground/5 backdrop-blur dark:bg-popover/92 dark:shadow-none"
				>
					<div className="border-b border-border/65 px-4 py-3">
						<div className="flex items-center justify-between gap-3">
							<div className="flex min-w-0 items-center gap-2 font-heading text-base leading-none font-medium">
								<Icon name="MusicNotes" className="h-4 w-4 text-primary" />
								<span className="truncate">{t("music_player_title")}</span>
							</div>
							<div className="flex items-center gap-1">
								<TooltipProvider>
									<PlayerIconButton
										label={t("music_player_close")}
										onClick={clear}
									>
										<Icon name="X" className="h-4 w-4" />
									</PlayerIconButton>
									<PlayerIconButton
										label={t("music_player_collapse")}
										onClick={closePanel}
									>
										<Icon name="CaretUp" className="h-4 w-4" />
									</PlayerIconButton>
								</TooltipProvider>
							</div>
						</div>
					</div>

					<div className="max-h-[calc(100vh-6.5rem)] overflow-y-auto overscroll-contain">
						<div className="px-4 py-4">
							<div className="flex min-w-0 gap-3">
								<MusicArtwork
									track={track}
									className="h-20 w-20 shrink-0 rounded-lg sm:h-24 sm:w-24"
								/>
								<div className="flex min-w-0 flex-1 flex-col justify-center">
									<AutoScrollText
										active={isPanelOpen}
										className="text-base font-semibold leading-6"
									>
										{displayTitle(track)}
									</AutoScrollText>
									<AutoScrollText
										active={isPanelOpen}
										className="mt-1 text-sm text-muted-foreground"
									>
										{displayArtist(track) ?? t("music_player_unknown_artist")}
									</AutoScrollText>
									<div className="mt-2 flex min-w-0 flex-wrap items-center gap-1.5">
										<Badge variant="outline">
											{formatPlaybackTime(duration)}
										</Badge>
										{track.size !== undefined ? (
											<Badge variant="outline">{formatBytes(track.size)}</Badge>
										) : null}
									</div>
								</div>
							</div>

							<div className="mt-4 space-y-2">
								<input
									type="range"
									min={0}
									max={100}
									step={0.1}
									value={progress}
									onChange={handleSeek}
									onBlur={endSeek}
									onKeyDown={beginSeek}
									onKeyUp={endSeek}
									onPointerCancel={endSeek}
									onPointerDown={beginSeek}
									onPointerUp={endSeek}
									aria-label={t("music_player_seek")}
									style={rangeFillStyle(progress, bufferedProgress)}
									className={cn(
										"h-2 w-full cursor-pointer appearance-none rounded-full accent-primary",
										duration <= 0 && "cursor-default opacity-60",
									)}
									disabled={duration <= 0}
								/>
								<div className="flex items-center justify-between text-[11px] tabular-nums text-muted-foreground">
									<span>{formatPlaybackTime(currentTime)}</span>
									<span>{formatPlaybackTime(duration)}</span>
								</div>
							</div>

							<TooltipProvider>
								<div className="mt-3 flex flex-wrap items-center justify-center gap-1">
									<PlayerIconButton
										active={playbackMode !== "repeat_queue"}
										label={modeLabel}
										onClick={() =>
											setPlaybackMode(nextPlaybackMode(playbackMode))
										}
									>
										<Icon
											name={playbackModeIcon(playbackMode)}
											className="h-4 w-4"
										/>
									</PlayerIconButton>
									<PlayerIconButton
										label={t("music_player_previous")}
										onClick={playPreviousTrack}
									>
										<Icon name="SkipBack" className="h-4 w-4" />
									</PlayerIconButton>
									<Button
										type="button"
										variant="default"
										size="icon"
										className="mx-1 h-11 w-11 rounded-full"
										onClick={togglePlayback}
										aria-label={
											isPlaying
												? t("music_player_pause")
												: t("music_player_play")
										}
									>
										<Icon
											name={isPlaying ? "Pause" : "Play"}
											className="h-5 w-5"
										/>
									</Button>
									<PlayerIconButton
										label={t("music_player_next")}
										onClick={playNextTrack}
									>
										<Icon name="SkipForward" className="h-4 w-4" />
									</PlayerIconButton>
									<div className="flex h-8 items-center gap-1 rounded-md px-1">
										<Icon
											name={volume === 0 ? "SpeakerSlash" : "SpeakerHigh"}
											className="h-4 w-4 text-muted-foreground"
										/>
										<input
											type="range"
											min={0}
											max={100}
											step={1}
											value={volumePercent}
											onChange={handleVolumeChange}
											aria-label={t("music_player_volume")}
											style={rangeFillStyle(volumePercent)}
											className="h-1.5 w-16 cursor-pointer appearance-none rounded-full accent-primary"
										/>
									</div>
								</div>
							</TooltipProvider>

							{error ? (
								<p className="mt-3 rounded-md border border-destructive/25 bg-destructive/8 px-3 py-2 text-xs text-destructive">
									{error}
								</p>
							) : null}

							<div className="mt-4 space-y-2 border-t border-border/65 pt-3">
								<button
									type="button"
									className="flex h-9 w-full items-center justify-between rounded-md px-2 text-sm font-medium transition hover:bg-muted/55 focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring/35"
									aria-controls={queuePanelId}
									aria-expanded={queueOpen}
									onClick={() => setQueueOpen((open) => !open)}
								>
									<span className="flex min-w-0 items-center gap-2">
										<Icon name="Queue" className="h-4 w-4 text-primary" />
										<span className="truncate">{t("music_player_queue")}</span>
										<Badge variant="outline">{queue.length}</Badge>
									</span>
									<Icon
										name={queueOpen ? "CaretUp" : "CaretDown"}
										className="h-4 w-4 text-muted-foreground"
									/>
								</button>
								<AnimatedCollapsible open={queueOpen}>
									<div id={queuePanelId} className="pb-1">
										<div className="max-h-72 overflow-y-auto rounded-md border border-border/55 bg-muted/12 overscroll-contain">
											<div className="space-y-1 p-2">
												{queue.map((queueTrack, index) => {
													const active = queueTrack.id === activeTrackId;
													return (
														<button
															key={queueTrack.id}
															ref={(node) => {
																if (active && node) {
																	activeQueueItemRef.current = node;
																}
															}}
															type="button"
															className={cn(
																"flex w-full min-w-0 items-center gap-3 rounded-md px-2 py-2 text-left transition hover:bg-muted/55",
																active &&
																	"bg-primary/10 text-primary hover:bg-primary/12",
															)}
															onClick={() => activateQueueTrack(queueTrack.id)}
														>
															<div
																className={cn(
																	"flex h-8 w-8 shrink-0 items-center justify-center rounded-md bg-muted text-xs tabular-nums text-muted-foreground",
																	active && "bg-primary/15 text-primary",
																)}
															>
																{active && isPlaying ? (
																	<Icon name="MusicNotes" className="h-4 w-4" />
																) : (
																	index + 1
																)}
															</div>
															<div className="min-w-0 flex-1">
																{active ? (
																	<AutoScrollText
																		active={queueOpen}
																		className="text-sm font-medium"
																	>
																		{displayTitle(queueTrack)}
																	</AutoScrollText>
																) : (
																	<span className="block truncate whitespace-nowrap text-sm font-medium">
																		{displayTitle(queueTrack)}
																	</span>
																)}
																{active ? (
																	<AutoScrollText
																		active={queueOpen}
																		className="text-xs text-muted-foreground"
																	>
																		{displayArtist(queueTrack) ??
																			t("music_player_unknown_artist")}
																	</AutoScrollText>
																) : (
																	<span className="block truncate whitespace-nowrap text-xs text-muted-foreground">
																		{displayArtist(queueTrack) ??
																			t("music_player_unknown_artist")}
																	</span>
																)}
															</div>
														</button>
													);
												})}
											</div>
										</div>
									</div>
								</AnimatedCollapsible>

								<button
									type="button"
									className="flex h-9 w-full items-center justify-between rounded-md px-2 text-sm font-medium transition hover:bg-muted/55 focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring/35"
									aria-controls={detailsPanelId}
									aria-expanded={detailsOpen}
									onClick={() => setDetailsOpen((open) => !open)}
								>
									<span className="flex min-w-0 items-center gap-2">
										<Icon name="Info" className="h-4 w-4 text-primary" />
										<span className="truncate">
											{t("music_player_details")}
										</span>
									</span>
									<Icon
										name={detailsOpen ? "CaretUp" : "CaretDown"}
										className="h-4 w-4 text-muted-foreground"
									/>
								</button>
								<AnimatedCollapsible open={detailsOpen}>
									<div
										id={detailsPanelId}
										className="space-y-3 rounded-md border border-border/55 bg-muted/12 p-3 text-sm"
									>
										<div>
											<div className="text-xs font-medium uppercase text-muted-foreground">
												{t("music_player_file_name")}
											</div>
											<AutoScrollText active={detailsOpen} className="mt-1">
												{track.name}
											</AutoScrollText>
										</div>
										<div>
											<div className="text-xs font-medium uppercase text-muted-foreground">
												{t("music_player_mime_type")}
											</div>
											<AutoScrollText active={detailsOpen} className="mt-1">
												{track.mimeType}
											</AutoScrollText>
										</div>
									</div>
								</AnimatedCollapsible>
							</div>
						</div>
					</div>
				</section>
			</div>
		</>
	);
}
