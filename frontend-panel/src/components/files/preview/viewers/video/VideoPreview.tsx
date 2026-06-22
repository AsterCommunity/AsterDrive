import Artplayer from "artplayer";
import { useEffect, useMemo, useRef, useState } from "react";
import { useTranslation } from "react-i18next";
import { resolveApiResourceUrl } from "@/lib/apiUrl";
import { logger } from "@/lib/logger";
import type { ResourcePath } from "@/lib/resourceRequest";
import type { ShareStreamSessionInfo } from "@/types/api";
import type { PreviewableFileLike } from "../../capabilities/types";
import { PreviewError } from "../../shared/PreviewError";
import { PreviewLoadingState } from "../../shared/PreviewLoadingState";
import {
	PreviewSurface,
	PreviewSurfaceContent,
} from "../../shared/PreviewSurface";
import { useVideoPreviewResource } from "./useVideoPreviewResource";

const DEFAULT_ASPECT_RATIO = 16 / 9;
const DIALOG_CHROME_HEIGHT_REM = 11;
const VIDEO_SURFACE_CLASS = "border-zinc-900/80 bg-zinc-950";
const VIDEO_CONTENT_CLASS = "flex items-center justify-center bg-zinc-950";

interface VideoPreviewProps {
	file: PreviewableFileLike;
	createMediaStreamSession?: () => Promise<ShareStreamSessionInfo>;
	resource: ResourcePath | null;
}

interface VideoStatus {
	aspectRatio: number;
	key: string;
	mediaFailed: boolean;
	playerFailed: boolean;
}

function initialVideoStatus(key: string): VideoStatus {
	return {
		aspectRatio: DEFAULT_ASPECT_RATIO,
		key,
		mediaFailed: false,
		playerFailed: false,
	};
}

function getPlayerLanguage(language: string) {
	return language.startsWith("zh") ? "zh-cn" : "en";
}

export function VideoPreview({
	file,
	createMediaStreamSession,
	resource,
}: VideoPreviewProps) {
	const { i18n, t } = useTranslation("files");
	const containerRef = useRef<HTMLDivElement | null>(null);
	const {
		error: resourceError,
		loading: resourceLoading,
		resolvedPath,
		resourceKey,
		retry: retryResource,
	} = useVideoPreviewResource({
		createMediaStreamSession,
		fileName: file.name,
		resource,
	});
	const [status, setStatus] = useState<VideoStatus>(() =>
		initialVideoStatus(resourceKey),
	);
	const currentStatus = useMemo(
		() =>
			status.key === resourceKey ? status : initialVideoStatus(resourceKey),
		[status, resourceKey],
	);
	const { aspectRatio, mediaFailed, playerFailed } = currentStatus;
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
		if (!videoSource) return;

		const metadataVideo = document.createElement("video");

		const handleLoadedMetadata = () => {
			if (metadataVideo.videoWidth <= 0 || metadataVideo.videoHeight <= 0)
				return;
			setStatus((prev) => ({
				...(prev.key === resourceKey ? prev : initialVideoStatus(resourceKey)),
				aspectRatio: metadataVideo.videoWidth / metadataVideo.videoHeight,
			}));
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
	}, [resourceKey, videoSource]);

	useEffect(() => {
		if (!containerRef.current || !videoSource || playerFailed || mediaFailed)
			return;

		let art: Artplayer | null = null;
		let videoElement: HTMLVideoElement | null = null;
		const handleVideoError = () => {
			setStatus({
				...initialVideoStatus(resourceKey),
				mediaFailed: true,
			});
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
			setStatus({
				...initialVideoStatus(resourceKey),
				playerFailed: true,
			});
		}

		return () => {
			videoElement?.removeEventListener("error", handleVideoError);
			art?.destroy(false);
		};
	}, [
		file.name,
		mediaFailed,
		playerFailed,
		playerLanguage,
		resourceKey,
		videoSource,
	]);

	if (resourceError || mediaFailed) {
		return (
			<PreviewSurface>
				<PreviewSurfaceContent>
					<PreviewError onRetry={resourceError ? retryResource : undefined} />
				</PreviewSurfaceContent>
			</PreviewSurface>
		);
	}

	if (resourceLoading || !videoSource) {
		return (
			<PreviewSurface>
				<PreviewSurfaceContent>
					<PreviewLoadingState text={t("loading_preview")} className="h-full" />
				</PreviewSurfaceContent>
			</PreviewSurface>
		);
	}

	if (playerFailed) {
		return (
			<PreviewSurface className={VIDEO_SURFACE_CLASS}>
				<PreviewSurfaceContent className={VIDEO_CONTENT_CLASS}>
					<div
						className="w-full overflow-hidden bg-zinc-950"
						style={previewFrameStyle}
					>
						{/* biome-ignore lint/a11y/useMediaCaption: user-uploaded media may not have captions available */}
						<video
							src={videoSource}
							aria-label={file.name}
							controls
							preload="metadata"
							onError={() =>
								setStatus({
									...initialVideoStatus(resourceKey),
									mediaFailed: true,
								})
							}
							className="block h-full w-full object-contain"
						/>
					</div>
				</PreviewSurfaceContent>
			</PreviewSurface>
		);
	}

	return (
		<PreviewSurface className={VIDEO_SURFACE_CLASS}>
			<PreviewSurfaceContent className={VIDEO_CONTENT_CLASS}>
				<div
					className="w-full overflow-hidden bg-zinc-950"
					style={previewFrameStyle}
				>
					<div ref={containerRef} className="h-full w-full" />
				</div>
			</PreviewSurfaceContent>
		</PreviewSurface>
	);
}
