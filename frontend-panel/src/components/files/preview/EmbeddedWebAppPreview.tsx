import type { ComponentProps, ReactNode } from "react";
import { cn } from "@/lib/utils";

export const EXTERNAL_WEB_APP_IFRAME_SANDBOX =
	"allow-scripts allow-forms allow-popups allow-downloads";
export const EXTERNAL_WEB_APP_SAME_ORIGIN_IFRAME_SANDBOX = `${EXTERNAL_WEB_APP_IFRAME_SANDBOX} allow-same-origin`;

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
		<div className="flex h-full min-h-[70vh] w-full flex-col gap-3">
			{headerStart || actions ? (
				<div className="flex flex-wrap items-center gap-2">
					{headerStart}
					{actions ? (
						<div
							className={cn(
								"flex flex-wrap items-center gap-2",
								headerStart ? "ml-auto" : "w-full justify-end",
							)}
						>
							{actions}
						</div>
					) : null}
				</div>
			) : null}
			<div className="relative min-h-0 flex-1 overflow-hidden rounded-xl border border-border/70 bg-card shadow-xs dark:shadow-none">
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
			</div>
		</div>
	);
}
