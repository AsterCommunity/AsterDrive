import type { ComponentProps, ReactNode } from "react";
import { cn } from "@/lib/utils";
import {
	PreviewSurface,
	PreviewSurfaceContent,
	PreviewSurfaceToolbar,
} from "./PreviewSurface";

export const EXTERNAL_WEB_APP_IFRAME_SANDBOX =
	"allow-scripts allow-forms allow-popups allow-downloads";
export const TRUSTED_DOCUMENT_VIEWER_IFRAME_SANDBOX = `${EXTERNAL_WEB_APP_IFRAME_SANDBOX} allow-same-origin allow-top-navigation allow-popups-to-escape-sandbox`;
export const TRUSTED_DOCUMENT_VIEWER_IFRAME_ALLOW =
	"autoplay; fullscreen; picture-in-picture; clipboard-read 'src'; clipboard-write 'src'";

interface EmbeddedWebAppPreviewProps {
	actions?: ReactNode;
	errorOverlay?: ReactNode;
	headerStart?: ReactNode;
	iframeAllow?: string;
	iframeClassName?: string;
	iframeHidden?: boolean;
	iframeName?: string;
	iframeReferrerPolicy?: ComponentProps<"iframe">["referrerPolicy"];
	iframeSandbox?: ComponentProps<"iframe">["sandbox"];
	loadingOverlay?: ReactNode;
	onLoad?: () => void;
	src: string | null;
	title: string;
}

export function EmbeddedWebAppPreview({
	actions,
	errorOverlay,
	headerStart,
	iframeAllow,
	iframeClassName,
	iframeHidden = false,
	iframeName,
	iframeReferrerPolicy = "same-origin",
	iframeSandbox = EXTERNAL_WEB_APP_IFRAME_SANDBOX,
	loadingOverlay,
	onLoad,
	src,
	title,
}: EmbeddedWebAppPreviewProps) {
	return (
		<PreviewSurface className="min-h-[70vh]">
			<PreviewSurfaceToolbar
				icon="Globe"
				label={title}
				meta={headerStart}
				actions={actions}
			/>
			<PreviewSurfaceContent className="relative">
				{src ? (
					<iframe
						key={src}
						title={title}
						src={src}
						name={iframeName}
						className={cn(
							"h-full w-full bg-background/80",
							iframeHidden && "pointer-events-none opacity-0",
							iframeClassName,
						)}
						allow={iframeAllow}
						referrerPolicy={iframeReferrerPolicy}
						sandbox={iframeSandbox}
						onLoad={onLoad}
					/>
				) : null}
				{loadingOverlay ? (
					<div className="absolute inset-0">{loadingOverlay}</div>
				) : null}
				{errorOverlay ? (
					<div className="absolute inset-0 flex items-center justify-center bg-card/90 p-6">
						{errorOverlay}
					</div>
				) : null}
			</PreviewSurfaceContent>
		</PreviewSurface>
	);
}
