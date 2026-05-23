import { useCallback, useEffect, useRef, useState } from "react";
import { useTranslation } from "react-i18next";
import { Button } from "@/components/ui/button";
import { Icon } from "@/components/ui/icon";
import { logger } from "@/lib/logger";
import {
	type MusicPlayerTrack,
	useMusicPlayerStore,
} from "@/stores/musicPlayerStore";
import type { ShareStreamSessionInfo } from "@/types/api";
import { PreviewError } from "./PreviewError";
import type { PreviewableFileLike } from "./types";

interface MusicPreviewProps {
	file: PreviewableFileLike;
	loadBackendMetadata?: MusicPlayerTrack["loadBackendMetadata"];
	mediaStreamLinkFactory?: () => Promise<ShareStreamSessionInfo>;
	path: string;
	thumbnailPath?: string;
}

function previewMusicTitle(name: string) {
	return name.replace(/\.[^.]+$/, "") || name;
}

export function MusicPreview({
	file,
	loadBackendMetadata,
	mediaStreamLinkFactory,
	path,
	thumbnailPath,
}: MusicPreviewProps) {
	const { t } = useTranslation("files");
	const currentTrackId = useMusicPlayerStore((state) => state.activeTrackId);
	const isPlaying = useMusicPlayerStore((state) => state.isPlaying);
	const playTrack = useMusicPlayerStore((state) => state.playTrack);
	const queue = useMusicPlayerStore((state) => state.queue);
	const requestPlayback = useMusicPlayerStore((state) => state.requestPlayback);
	const [streamLinkFailed, setStreamLinkFailed] = useState(false);
	const [starting, setStarting] = useState(false);
	const mountedRef = useRef(true);
	const startRequestIdRef = useRef(0);
	const trackId = `${file.name}:${file.size ?? "unknown"}:${file.mime_type}:${path}`;
	const isCurrentTrack =
		currentTrackId === trackId && queue.some((track) => track.id === trackId);

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
					metadata: {
						title: previewMusicTitle(file.name),
					},
					name: file.name,
					mimeType: file.mime_type,
					path: link.path,
					size: file.size,
					expiresAt: link.expires_at || undefined,
					loadBackendMetadata,
					refreshStreamLink: mediaStreamLinkFactory,
					thumbnail: thumbnailPath
						? {
								file: {
									file_category: file.file_category ?? "audio",
									id: file.id ?? -1,
									mime_type: file.mime_type,
									name: file.name,
								},
								path: thumbnailPath,
							}
						: undefined,
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
		file.file_category,
		file.id,
		file.size,
		loadBackendMetadata,
		mediaStreamLinkFactory,
		path,
		playTrack,
		thumbnailPath,
		trackId,
	]);

	if (streamLinkFailed) {
		return <PreviewError />;
	}

	const statusText = isCurrentTrack
		? isPlaying
			? t("music_preview_playing")
			: t("music_preview_ready")
		: t("music_preview_idle");

	return (
		<div className="flex min-h-[50vh] items-center justify-center px-6">
			<div className="flex w-full max-w-xl flex-col items-center gap-4 rounded-lg border border-border/70 bg-card/70 px-6 py-8 text-center shadow-sm dark:bg-card/35">
				<div className="flex size-14 items-center justify-center rounded-lg bg-primary/10 text-primary">
					<Icon name="FileAudio" className="size-7" />
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
						className={starting ? "size-4 animate-spin" : "size-4"}
					/>
					{starting
						? t("loading_preview")
						: isCurrentTrack && isPlaying
							? t("music_preview_playing")
							: isCurrentTrack
								? t("music_preview_resume")
								: t("music_preview_play")}
				</Button>
			</div>
		</div>
	);
}
