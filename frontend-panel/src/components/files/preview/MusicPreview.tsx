import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import { useTranslation } from "react-i18next";
import { Button } from "@/components/ui/button";
import { Icon } from "@/components/ui/icon";
import { logger } from "@/lib/logger";
import { inferMusicMetadata } from "@/lib/musicPlayer";
import {
	type MusicPlayerTrack,
	type MusicTrackMetadata,
	useMusicPlayerStore,
} from "@/stores/musicPlayerStore";
import type { ShareStreamSessionInfo } from "@/types/api";
import { PreviewError } from "./PreviewError";
import {
	PreviewSurface,
	PreviewSurfaceContent,
	PreviewSurfaceToolbar,
} from "./PreviewSurface";
import type { PreviewableFileLike } from "./types";

interface MusicPreviewProps {
	file: PreviewableFileLike;
	loadBackendMetadata?: MusicPlayerTrack["loadBackendMetadata"];
	mediaStreamLinkFactory?: () => Promise<ShareStreamSessionInfo>;
	path: string;
	thumbnailPath?: string;
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
	const [loadedMetadata, setLoadedMetadata] = useState<{
		metadata: MusicTrackMetadata;
		trackId: string;
	} | null>(null);
	const mountedRef = useRef(true);
	const startRequestIdRef = useRef(0);
	const trackId = `${file.name}:${file.size ?? "unknown"}:${file.mime_type}:${path}`;
	const isCurrentTrack =
		currentTrackId === trackId && queue.some((track) => track.id === trackId);
	const fallbackMetadata = useMemo(
		() =>
			inferMusicMetadata({
				file_category: file.file_category,
				id: file.id ?? -1,
				mime_type: file.mime_type,
				name: file.name,
				size: file.size,
			}),
		[file.file_category, file.id, file.mime_type, file.name, file.size],
	);
	const trackMetadata =
		loadedMetadata?.trackId === trackId
			? loadedMetadata.metadata
			: fallbackMetadata;
	const trackTitle = trackMetadata.title?.trim() || file.name;

	useEffect(() => {
		mountedRef.current = true;
		return () => {
			mountedRef.current = false;
			startRequestIdRef.current += 1;
		};
	}, []);

	useEffect(() => {
		if (!loadBackendMetadata) return;

		const controller = new AbortController();
		void loadBackendMetadata(controller.signal)
			.then((metadata) => {
				if (controller.signal.aborted || !metadata) return;
				setLoadedMetadata({ metadata, trackId });
			})
			.catch(() => {
				// Backend media metadata is best-effort. Playback must stay immediate.
			});

		return () => controller.abort();
	}, [loadBackendMetadata, trackId]);

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
					metadata: trackMetadata,
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
		trackMetadata,
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
		<PreviewSurface className="min-h-[50vh]">
			<PreviewSurfaceToolbar
				icon="FileAudio"
				label={trackTitle}
				meta={statusText}
			/>
			<PreviewSurfaceContent>
				<div className="flex h-full min-h-[18rem] items-center justify-center px-6">
					<div className="flex w-full max-w-sm flex-col items-center gap-4 text-center">
						<div className="flex size-14 items-center justify-center rounded-lg bg-primary/10 text-primary">
							<Icon name="FileAudio" className="size-7" />
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
			</PreviewSurfaceContent>
		</PreviewSurface>
	);
}
