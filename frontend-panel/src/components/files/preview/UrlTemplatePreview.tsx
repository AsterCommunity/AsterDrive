import { type ReactNode, useEffect, useMemo, useState } from "react";
import { useTranslation } from "react-i18next";
import { EmptyState } from "@/components/common/EmptyState";
import { Button } from "@/components/ui/button";
import { Icon } from "@/components/ui/icon";
import type { PreviewLinkInfo } from "@/types/api";
import {
	EmbeddedWebAppPreview,
	EXTERNAL_WEB_APP_IFRAME_SANDBOX,
	TRUSTED_DOCUMENT_VIEWER_IFRAME_ALLOW,
	TRUSTED_DOCUMENT_VIEWER_IFRAME_SANDBOX,
} from "./EmbeddedWebAppPreview";
import { PreviewLoadingState } from "./PreviewLoadingState";
import { PreviewSurface, PreviewSurfaceContent } from "./PreviewSurface";
import {
	type ResolvedVideoBrowserTarget,
	resolveUrlTemplateTarget,
	type VideoBrowserFileContext,
} from "./video-browser-config";

interface UrlTemplatePreviewProps {
	createPreviewLink?: () => Promise<PreviewLinkInfo>;
	downloadPath: string;
	file: VideoBrowserFileContext;
	label: string;
	optionKey?: string;
	rawConfig: Record<string, unknown> | null | undefined;
}

const TRUSTED_DOCUMENT_VIEWER_URL_TEMPLATE_KEYS = new Set([
	"builtin.office_google",
	"builtin.office_microsoft",
]);

interface UrlTemplatePreviewState {
	requestKey: UrlTemplatePreviewRequestKey;
	target: ResolvedVideoBrowserTarget | null;
}

interface UrlTemplatePreviewRequestKey {
	createPreviewLink: UrlTemplatePreviewProps["createPreviewLink"];
	downloadPath: string;
	file: VideoBrowserFileContext;
	label: string;
	rawConfig: UrlTemplatePreviewProps["rawConfig"];
}

function UrlTemplatePreviewStatePane({
	action,
	description,
	icon,
	title,
}: {
	action?: ReactNode;
	description: string;
	icon: ReactNode;
	title: string;
}) {
	return (
		<PreviewSurface>
			<PreviewSurfaceContent>
				<EmptyState
					icon={icon}
					title={title}
					description={description}
					action={action}
				/>
			</PreviewSurfaceContent>
		</PreviewSurface>
	);
}

export function UrlTemplatePreview({
	createPreviewLink,
	downloadPath,
	file,
	label,
	optionKey,
	rawConfig,
}: UrlTemplatePreviewProps) {
	const { t } = useTranslation("files");
	const [previewState, setPreviewState] =
		useState<UrlTemplatePreviewState | null>(null);
	const requestKey = useMemo<UrlTemplatePreviewRequestKey>(
		() => ({ createPreviewLink, downloadPath, file, label, rawConfig }),
		[createPreviewLink, downloadPath, file, label, rawConfig],
	);

	useEffect(() => {
		let cancelled = false;

		void resolveUrlTemplateTarget(
			file,
			downloadPath,
			label,
			rawConfig,
			createPreviewLink,
		)
			.then((resolvedTarget) => {
				if (cancelled) return;
				setPreviewState({ requestKey, target: resolvedTarget });
			})
			.catch(() => {
				if (cancelled) return;
				setPreviewState({ requestKey, target: null });
			});

		return () => {
			cancelled = true;
		};
	}, [createPreviewLink, downloadPath, file, label, rawConfig, requestKey]);

	const isLoading = previewState?.requestKey !== requestKey;
	const target = isLoading ? null : previewState.target;

	const openTarget = () => {
		if (!target) return;
		window.open(target.url, "_blank", "noopener,noreferrer");
	};

	if (isLoading) {
		return (
			<PreviewLoadingState
				text={t("loading_preview")}
				className="h-full min-h-[16rem]"
			/>
		);
	}

	if (!target) {
		return (
			<UrlTemplatePreviewStatePane
				icon={<Icon name="Globe" className="size-10" />}
				title={t("url_template_unavailable")}
				description={t("url_template_unavailable_desc")}
			/>
		);
	}

	if (target.mode === "new_tab") {
		return (
			<UrlTemplatePreviewStatePane
				icon={<Icon name="ArrowSquareOut" className="size-10" />}
				title={target.label}
				description={t("url_template_external_desc", { label: target.label })}
				action={
					<Button variant="outline" onClick={openTarget}>
						<Icon name="ArrowSquareOut" className="mr-2 size-4" />
						{t("url_template_open", { label: target.label })}
					</Button>
				}
			/>
		);
	}

	return (
		<EmbeddedWebAppPreview
			title={target.label}
			src={target.url}
			actions={
				<Button variant="outline" size="sm" onClick={openTarget}>
					<Icon name="ArrowSquareOut" className="mr-2 size-4" />
					{t("url_template_open", { label: target.label })}
				</Button>
			}
			iframeAllow={
				optionKey && TRUSTED_DOCUMENT_VIEWER_URL_TEMPLATE_KEYS.has(optionKey)
					? TRUSTED_DOCUMENT_VIEWER_IFRAME_ALLOW
					: "autoplay; fullscreen; picture-in-picture"
			}
			iframeReferrerPolicy="same-origin"
			iframeSandbox={
				optionKey && TRUSTED_DOCUMENT_VIEWER_URL_TEMPLATE_KEYS.has(optionKey)
					? TRUSTED_DOCUMENT_VIEWER_IFRAME_SANDBOX
					: EXTERNAL_WEB_APP_IFRAME_SANDBOX
			}
		/>
	);
}
