import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import { useTranslation } from "react-i18next";
import { Button } from "@/components/ui/button";
import { Icon } from "@/components/ui/icon";
import { logger } from "@/lib/logger";
import { useAudioPlayerStore } from "@/stores/audioPlayerStore";
import type { ShareStreamSessionInfo } from "@/types/api";
import { PreviewError } from "./PreviewError";
import type { PreviewableFileLike } from "./types";

interface AudioPreviewProps {
	file: PreviewableFileLike;
	mediaStreamLinkFactory?: () => Promise<ShareStreamSessionInfo>;
	path: string;
}

export function AudioPreview({
	file,
	mediaStreamLinkFactory,
	path,
}: AudioPreviewProps) {
	const { t } = useTranslation("files");
	const currentTrack = useAudioPlayerStore((state) => state.track);
	const isPlaying = useAudioPlayerStore((state) => state.isPlaying);
	const playTrack = useAudioPlayerStore((state) => state.playTrack);
	const requestPlayback = useAudioPlayerStore((state) => state.requestPlayback);
	const [streamLinkFailed, setStreamLinkFailed] = useState(false);
	const [starting, setStarting] = useState(false);
	const mountedRef = useRef(true);
	const startRequestIdRef = useRef(0);
	const trackId = useMemo(
		() => `${file.name}:${file.size ?? "unknown"}:${file.mime_type}:${path}`,
		[file.mime_type, file.name, file.size, path],
	);
	const isCurrentTrack = currentTrack?.id === trackId;

	useEffect(() => {
		mountedRef.current = true;
		return () => {
			mountedRef.current = false;
			startRequestIdRef.current += 1;
		};
	}, []);

	const startPlayback = useCallback(() => {
		const requestId = startRequestIdRef.current + 1;
		startRequestIdRef.current = requestId;
		setStreamLinkFailed(false);
		setStarting(true);

		const resolveLink = mediaStreamLinkFactory
			? mediaStreamLinkFactory
			: async () => ({
					expires_at: "",
					path,
				});

		resolveLink()
			.then((link) => {
				if (!mountedRef.current || startRequestIdRef.current !== requestId)
					return;
				playTrack({
					id: trackId,
					name: file.name,
					mimeType: file.mime_type,
					path: link.path,
					size: file.size,
					expiresAt: link.expires_at || undefined,
					refreshStreamLink: mediaStreamLinkFactory,
				});
			})
			.catch((error) => {
				if (!mountedRef.current || startRequestIdRef.current !== requestId)
					return;
				logger.warn("audio stream session creation failed", file.name, error);
				setStreamLinkFailed(true);
			})
			.finally(() => {
				if (mountedRef.current && startRequestIdRef.current === requestId) {
					setStarting(false);
				}
			});
	}, [
		file.mime_type,
		file.name,
		file.size,
		mediaStreamLinkFactory,
		path,
		playTrack,
		trackId,
	]);

	if (streamLinkFailed) {
		return <PreviewError />;
	}

	const statusText = isCurrentTrack
		? isPlaying
			? t("audio_preview_playing")
			: t("audio_preview_ready")
		: t("audio_preview_idle");

	return (
		<div className="flex min-h-[50vh] items-center justify-center px-6">
			<div className="flex w-full max-w-xl flex-col items-center gap-4 rounded-lg border border-border/70 bg-card/70 px-6 py-8 text-center shadow-sm dark:bg-card/35">
				<div className="flex h-14 w-14 items-center justify-center rounded-lg bg-primary/10 text-primary">
					<Icon name="FileAudio" className="h-7 w-7" />
				</div>
				<div className="min-w-0 space-y-1">
					<p className="max-w-md truncate text-sm font-medium">{file.name}</p>
					<p className="text-sm text-muted-foreground">{statusText}</p>
				</div>
				<Button
					type="button"
					variant="default"
					size="sm"
					onClick={() => {
						if (isCurrentTrack) {
							requestPlayback();
							return;
						}
						startPlayback();
					}}
					disabled={starting || (isCurrentTrack && isPlaying)}
				>
					<Icon
						name={starting ? "Spinner" : "Play"}
						className={starting ? "h-4 w-4 animate-spin" : "h-4 w-4"}
					/>
					{starting
						? t("loading_preview")
						: isCurrentTrack && isPlaying
							? t("audio_preview_playing")
							: isCurrentTrack
								? t("audio_preview_resume")
								: t("audio_preview_play")}
				</Button>
			</div>
		</div>
	);
}
