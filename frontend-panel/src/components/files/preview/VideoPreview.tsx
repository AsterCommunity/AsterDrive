import Artplayer from "artplayer";
import { useEffect, useMemo, useRef, useState } from "react";
import { useTranslation } from "react-i18next";
import { resolveApiResourceUrl } from "@/lib/apiUrl";
import { logger } from "@/lib/logger";
import type { ShareStreamSessionInfo } from "@/types/api";
import { PreviewError } from "./PreviewError";
import { PreviewLoadingState } from "./PreviewLoadingState";
import {
	PreviewSurface,
	PreviewSurfaceContent,
	PreviewSurfaceToolbar,
} from "./PreviewSurface";
import type { PreviewableFileLike } from "./types";

const DEFAULT_ASPECT_RATIO = 16 / 9;
const DIALOG_CHROME_HEIGHT_REM = 11;

interface VideoPreviewProps {
	file: PreviewableFileLike;
	mediaStreamLinkFactory?: () => Promise<ShareStreamSessionInfo>;
	path: string;
}

function getPlayerLanguage(language: string) {
	return language.startsWith("zh") ? "zh-cn" : "en";
}

export function VideoPreview({
	file,
	mediaStreamLinkFactory,
	path,
}: VideoPreviewProps) {
	const { i18n, t } = useTranslation("files");
	const containerRef = useRef<HTMLDivElement | null>(null);
	const [resolvedPath, setResolvedPath] = useState<string | null>(
		mediaStreamLinkFactory ? null : path,
	);
	const [streamLinkFailed, setStreamLinkFailed] = useState(false);
	const [playerFailed, setPlayerFailed] = useState(false);
	const [mediaFailed, setMediaFailed] = useState(false);
	const [aspectRatio, setAspectRatio] = useState(DEFAULT_ASPECT_RATIO);
	const videoSource = useMemo(
		() => (resolvedPath ? resolveApiResourceUrl(resolvedPath) : null),
		[resolvedPath],
	);

	const playerLanguage = useMemo(
		() => getPlayerLanguage(i18n.language),
		[i18n.language],
	);
	const previewFrameStyle = useMemo(
		() => ({
			aspectRatio: String(aspectRatio),
			maxWidth: `min(100%, calc((90vh - ${DIALOG_CHROME_HEIGHT_REM}rem) * ${aspectRatio}))`,
		}),
		[aspectRatio],
	);

	useEffect(() => {
		let cancelled = false;
		setStreamLinkFailed(false);
		setPlayerFailed(false);
		setMediaFailed(false);
		setAspectRatio(DEFAULT_ASPECT_RATIO);

		if (!mediaStreamLinkFactory) {
			setResolvedPath(path);
			return () => {
				cancelled = true;
			};
		}

		setResolvedPath(null);
		mediaStreamLinkFactory()
			.then((link) => {
				if (cancelled) return;
				setResolvedPath(link.path);
			})
			.catch((error) => {
				if (cancelled) return;
				logger.warn("media stream session creation failed", file.name, error);
				setStreamLinkFailed(true);
			});

		return () => {
			cancelled = true;
		};
	}, [file.name, path, mediaStreamLinkFactory]);

	useEffect(() => {
		if (!videoSource) return;

		setPlayerFailed(false);
		setMediaFailed(false);
		setAspectRatio(DEFAULT_ASPECT_RATIO);

		const metadataVideo = document.createElement("video");

		const handleLoadedMetadata = () => {
			if (metadataVideo.videoWidth <= 0 || metadataVideo.videoHeight <= 0)
				return;
			setAspectRatio(metadataVideo.videoWidth / metadataVideo.videoHeight);
		};

		metadataVideo.preload = "metadata";
		metadataVideo.src = videoSource;
		metadataVideo.addEventListener("loadedmetadata", handleLoadedMetadata);
		metadataVideo.load();

		return () => {
			metadataVideo.removeEventListener("loadedmetadata", handleLoadedMetadata);
			metadataVideo.removeAttribute("src");
			metadataVideo.load();
		};
	}, [videoSource]);

	useEffect(() => {
		if (!containerRef.current || !videoSource || playerFailed || mediaFailed)
			return;

		let art: Artplayer | null = null;
		let videoElement: HTMLVideoElement | null = null;
		const handleVideoError = () => {
			setMediaFailed(true);
		};

		try {
			art = new Artplayer({
				container: containerRef.current,
				url: videoSource,
				lang: playerLanguage,
				fullscreen: true,
				fullscreenWeb: true,
				pip: true,
				setting: true,
				playbackRate: true,
				miniProgressBar: false,
				mutex: true,
				hotkey: true,
				playsInline: true,
				airplay: true,
				moreVideoAttr: {
					preload: "metadata",
				},
			});
			videoElement = art.template.$video;
			videoElement.style.objectFit = "contain";
			videoElement.addEventListener("error", handleVideoError);
		} catch (playerError) {
			logger.warn("artplayer init failed", file.name, playerError);
			setPlayerFailed(true);
		}

		return () => {
			videoElement?.removeEventListener("error", handleVideoError);
			art?.destroy(false);
		};
	}, [file.name, mediaFailed, playerFailed, playerLanguage, videoSource]);

	if (streamLinkFailed || mediaFailed) {
		return (
			<PreviewSurface>
				<PreviewSurfaceToolbar
					icon="FileVideo"
					label={t("preview_mode_video")}
				/>
				<PreviewSurfaceContent>
					<PreviewError />
				</PreviewSurfaceContent>
			</PreviewSurface>
		);
	}

	if (!videoSource) {
		return (
			<PreviewSurface>
				<PreviewSurfaceToolbar
					icon="FileVideo"
					label={t("preview_mode_video")}
				/>
				<PreviewSurfaceContent>
					<PreviewLoadingState text={t("loading_preview")} className="h-full" />
				</PreviewSurfaceContent>
			</PreviewSurface>
		);
	}

	if (playerFailed) {
		return (
			<PreviewSurface>
				<PreviewSurfaceToolbar
					icon="FileVideo"
					label={t("preview_mode_video")}
				/>
				<PreviewSurfaceContent className="flex items-center justify-center p-4">
					<div
						className="w-full overflow-hidden rounded-lg bg-zinc-950"
						style={previewFrameStyle}
					>
						{/* biome-ignore lint/a11y/useMediaCaption: user-uploaded media may not have captions available */}
						<video
							src={videoSource}
							aria-label={file.name}
							controls
							preload="metadata"
							onError={() => setMediaFailed(true)}
							className="block h-full w-full object-contain"
						/>
					</div>
				</PreviewSurfaceContent>
			</PreviewSurface>
		);
	}

	return (
		<PreviewSurface>
			<PreviewSurfaceToolbar icon="FileVideo" label={t("preview_mode_video")} />
			<PreviewSurfaceContent className="flex items-center justify-center p-4">
				<div
					className="w-full overflow-hidden rounded-lg bg-zinc-950"
					style={previewFrameStyle}
				>
					<div ref={containerRef} className="h-full w-full" />
				</div>
			</PreviewSurfaceContent>
		</PreviewSurface>
	);
}
