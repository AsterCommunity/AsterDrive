import Artplayer from "artplayer";
import { useEffect, useMemo, useRef, useState } from "react";
import { useTranslation } from "react-i18next";
import { resolveApiResourceUrl } from "@/lib/apiUrl";
import { prepareAuthenticatedResource } from "@/lib/authenticatedResource";
import { logger } from "@/lib/logger";
import type { ShareStreamSessionInfo } from "@/types/api";
import { PreviewError } from "./PreviewError";
import { PreviewLoadingState } from "./PreviewLoadingState";
import { PreviewSurface, PreviewSurfaceContent } from "./PreviewSurface";
import type { PreviewableFileLike } from "./types";

const DEFAULT_ASPECT_RATIO = 16 / 9;
const DIALOG_CHROME_HEIGHT_REM = 11;
const VIDEO_SURFACE_CLASS = "border-zinc-900/80 bg-zinc-950";
const VIDEO_CONTENT_CLASS = "flex items-center justify-center bg-zinc-950";

interface VideoPreviewProps {
	file: PreviewableFileLike;
	mediaStreamLinkFactory?: () => Promise<ShareStreamSessionInfo>;
	path: string;
}

interface VideoStatus {
	aspectRatio: number;
	key: string;
	mediaFailed: boolean;
	playerFailed: boolean;
	streamLinkFailed: boolean;
}

function initialVideoStatus(key: string): VideoStatus {
	return {
		aspectRatio: DEFAULT_ASPECT_RATIO,
		key,
		mediaFailed: false,
		playerFailed: false,
		streamLinkFailed: false,
	};
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
	const [resourceState, setResourceState] = useState(() => ({
		inputs: { mediaStreamLinkFactory, path },
		version: 0,
	}));
	const [resolvedResource, setResolvedResource] = useState<{
		key: string;
		path: string;
	} | null>(null);
	if (
		resourceState.inputs.path !== path ||
		resourceState.inputs.mediaStreamLinkFactory !== mediaStreamLinkFactory
	) {
		setResourceState({
			inputs: { mediaStreamLinkFactory, path },
			version: resourceState.version + 1,
		});
	}
	const currentResourceVersion =
		resourceState.inputs.path === path &&
		resourceState.inputs.mediaStreamLinkFactory === mediaStreamLinkFactory
			? resourceState.version
			: resourceState.version + 1;
	const resourceKey = `${path}:${mediaStreamLinkFactory ? "stream" : "direct"}:${currentResourceVersion}`;
	const [status, setStatus] = useState<VideoStatus>(() =>
		initialVideoStatus(resourceKey),
	);
	const currentStatus = useMemo(
		() =>
			status.key === resourceKey ? status : initialVideoStatus(resourceKey),
		[status, resourceKey],
	);
	const { aspectRatio, mediaFailed, playerFailed, streamLinkFailed } =
		currentStatus;
	const resolvedPath =
		resolvedResource?.key === resourceKey ? resolvedResource.path : null;
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

		const resolveDirectPath = async () => {
			await prepareAuthenticatedResource(path);
			return path;
		};

		const resolveLink = mediaStreamLinkFactory
			? async () => (await mediaStreamLinkFactory()).path
			: resolveDirectPath;

		resolveLink()
			.then((nextPath) => {
				if (cancelled) return;
				setResolvedResource({ key: resourceKey, path: nextPath });
			})
			.catch((error) => {
				if (cancelled) return;
				logger.warn(
					mediaStreamLinkFactory
						? "media stream session creation failed"
						: "media resource preparation failed",
					file.name,
					error,
				);
				setStatus({
					...initialVideoStatus(resourceKey),
					streamLinkFailed: true,
				});
			});

		return () => {
			cancelled = true;
		};
	}, [file.name, path, mediaStreamLinkFactory, resourceKey]);

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

	if (streamLinkFailed || mediaFailed) {
		return (
			<PreviewSurface>
				<PreviewSurfaceContent>
					<PreviewError />
				</PreviewSurfaceContent>
			</PreviewSurface>
		);
	}

	if (!videoSource) {
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
