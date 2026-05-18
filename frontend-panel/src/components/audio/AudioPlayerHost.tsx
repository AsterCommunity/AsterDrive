import type { ChangeEvent } from "react";
import { useEffect, useMemo, useRef, useState } from "react";
import { useTranslation } from "react-i18next";
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
import { cn } from "@/lib/utils";
import { useAudioPlayerStore } from "@/stores/audioPlayerStore";

const STREAM_REFRESH_LEAD_MS = 2 * 60 * 1000;
const STREAM_REFRESH_MIN_DELAY_MS = 10 * 1000;

function formatPlaybackTime(seconds: number) {
	if (!Number.isFinite(seconds) || seconds < 0) {
		return "0:00";
	}

	const totalSeconds = Math.floor(seconds);
	const minutes = Math.floor(totalSeconds / 60);
	const remainingSeconds = totalSeconds % 60;
	return `${minutes}:${remainingSeconds.toString().padStart(2, "0")}`;
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

export function AudioPlayerHost() {
	const { t } = useTranslation("files");
	const audioRef = useRef<HTMLAudioElement | null>(null);
	const [currentTime, setCurrentTime] = useState(0);
	const [duration, setDuration] = useState(0);
	const error = useAudioPlayerStore((state) => state.error);
	const isPlaying = useAudioPlayerStore((state) => state.isPlaying);
	const playRequested = useAudioPlayerStore((state) => state.playRequested);
	const track = useAudioPlayerStore((state) => state.track);
	const clear = useAudioPlayerStore((state) => state.clear);
	const requestPlayback = useAudioPlayerStore((state) => state.requestPlayback);
	const setError = useAudioPlayerStore((state) => state.setError);
	const setPlaying = useAudioPlayerStore((state) => state.setPlaying);
	const setPlaybackRequested = useAudioPlayerStore(
		(state) => state.setPlaybackRequested,
	);
	const updateTrackSource = useAudioPlayerStore(
		(state) => state.updateTrackSource,
	);
	const source = useMemo(
		() => (track ? resolveApiResourceUrl(track.path) : null),
		[track],
	);
	const trackKey = track ? `${track.id}:${track.path}` : null;
	const progress =
		duration > 0 && Number.isFinite(duration)
			? Math.min(100, Math.max(0, (currentTime / duration) * 100))
			: 0;

	useEffect(() => {
		if (!trackKey) return;
		setCurrentTime(0);
		setDuration(0);
	}, [trackKey]);

	useEffect(() => {
		if (!track?.refreshStreamLink) return;

		const delay = sessionRefreshDelay(track.expiresAt);
		if (delay === null) return;

		const timer = window.setTimeout(() => {
			track
				.refreshStreamLink?.()
				.then((link) => {
					updateTrackSource(track.id, link);
				})
				.catch((refreshError) => {
					logger.warn(
						"audio stream session refresh failed",
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

		if (!playRequested) {
			audio.pause();
			return;
		}

		void audio.play().catch((playError) => {
			logger.warn("audio playback start failed", track?.name, playError);
			setPlaybackRequested(false);
			setPlaying(false);
		});
	}, [playRequested, setPlaybackRequested, setPlaying, source, track?.name]);

	if (!track || !source) {
		return null;
	}

	const togglePlayback = () => {
		if (isPlaying) {
			audioRef.current?.pause();
			setPlaybackRequested(false);
			return;
		}

		requestPlayback();
	};

	const handleSeek = (event: ChangeEvent<HTMLInputElement>) => {
		const audio = audioRef.current;
		if (!audio || duration <= 0) return;

		const nextTime = (Number(event.currentTarget.value) / 100) * duration;
		audio.currentTime = nextTime;
		setCurrentTime(nextTime);
	};

	return (
		<div className="pointer-events-none fixed inset-x-0 bottom-0 z-40 px-3 pb-3 sm:px-4">
			<div className="pointer-events-auto mx-auto flex max-w-5xl items-center gap-3 rounded-lg border border-border/80 bg-background/95 px-3 py-2 shadow-lg shadow-black/10 backdrop-blur supports-[backdrop-filter]:bg-background/85 dark:shadow-black/35">
				{/* biome-ignore lint/a11y/useMediaCaption: user-uploaded media may not have captions available */}
				<audio
					ref={audioRef}
					src={source}
					preload="metadata"
					onCanPlay={() => setError(null)}
					onDurationChange={(event) =>
						setDuration(event.currentTarget.duration || 0)
					}
					onEnded={() => {
						setPlaybackRequested(false);
						setPlaying(false);
					}}
					onError={() => {
						setError(t("audio_player_load_failed"));
						setPlaybackRequested(false);
						setPlaying(false);
					}}
					onLoadedMetadata={(event) => {
						setDuration(event.currentTarget.duration || 0);
					}}
					onPause={() => setPlaying(false)}
					onPlay={() => {
						setError(null);
						setPlaying(true);
						setPlaybackRequested(true);
					}}
					onTimeUpdate={(event) =>
						setCurrentTime(event.currentTarget.currentTime || 0)
					}
				/>
				<TooltipProvider>
					<Tooltip>
						<TooltipTrigger
							render={
								<Button
									type="button"
									variant="secondary"
									size="icon"
									onClick={togglePlayback}
									aria-label={
										isPlaying ? t("audio_player_pause") : t("audio_player_play")
									}
								/>
							}
						>
							<Icon name={isPlaying ? "Pause" : "Play"} className="h-4 w-4" />
						</TooltipTrigger>
						<TooltipContent>
							{isPlaying ? t("audio_player_pause") : t("audio_player_play")}
						</TooltipContent>
					</Tooltip>
				</TooltipProvider>
				<div className="min-w-0 flex-1">
					<div className="flex items-center gap-2">
						<p className="min-w-0 truncate text-sm font-medium">{track.name}</p>
						{track.size !== undefined ? (
							<span className="hidden shrink-0 text-xs text-muted-foreground sm:inline">
								{formatBytes(track.size)}
							</span>
						) : null}
					</div>
					<div className="mt-1 flex items-center gap-2">
						<span className="w-9 text-right text-[11px] tabular-nums text-muted-foreground">
							{formatPlaybackTime(currentTime)}
						</span>
						<input
							type="range"
							min={0}
							max={100}
							step={0.1}
							value={progress}
							onChange={handleSeek}
							aria-label={t("audio_player_seek")}
							className={cn(
								"h-1.5 min-w-0 flex-1 cursor-pointer appearance-none rounded-full bg-muted accent-primary",
								duration <= 0 && "cursor-default opacity-60",
							)}
							disabled={duration <= 0}
						/>
						<span className="w-9 text-[11px] tabular-nums text-muted-foreground">
							{formatPlaybackTime(duration)}
						</span>
					</div>
					{error ? (
						<p className="mt-1 text-xs text-destructive">{error}</p>
					) : null}
				</div>
				<TooltipProvider>
					<Tooltip>
						<TooltipTrigger
							render={
								<Button
									type="button"
									variant="ghost"
									size="icon-sm"
									onClick={clear}
									aria-label={t("audio_player_close")}
								/>
							}
						>
							<Icon name="X" className="h-4 w-4" />
						</TooltipTrigger>
						<TooltipContent>{t("audio_player_close")}</TooltipContent>
					</Tooltip>
				</TooltipProvider>
			</div>
		</div>
	);
}
