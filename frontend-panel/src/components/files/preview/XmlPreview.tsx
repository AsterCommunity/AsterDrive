import { useMemo } from "react";
import { useTranslation } from "react-i18next";
import xmlFormatter from "xml-formatter";
import { ScrollArea } from "@/components/ui/scroll-area";
import { useTextContent } from "@/hooks/useTextContent";
import type { ResourcePath } from "@/lib/resourceRequest";
import { PreviewError } from "./PreviewError";
import { PreviewLoadingState } from "./PreviewLoadingState";
import {
	PreviewSurface,
	PreviewSurfaceContent,
	PreviewSurfaceMessage,
	PreviewSurfaceToolbar,
} from "./PreviewSurface";

interface XmlPreviewProps {
	resource: ResourcePath;
	mode: "formatted";
}

export function XmlPreview({ resource }: XmlPreviewProps) {
	const { t } = useTranslation("files");
	const { content, loading, error, reload } = useTextContent(resource);

	const formatted = useMemo(() => {
		if (!content) return null;
		const doc = new DOMParser().parseFromString(content, "application/xml");
		if (doc.querySelector("parsererror")) return null;
		try {
			return xmlFormatter(content, {
				indentation: "  ",
				lineSeparator: "\n",
				collapseContent: false,
			});
		} catch {
			return null;
		}
	}, [content]);

	if (loading) {
		return (
			<PreviewLoadingState text={t("loading_preview")} className="h-full" />
		);
	}

	if (error || content === null) {
		return <PreviewError onRetry={() => void reload()} />;
	}

	if (!formatted) {
		return (
			<PreviewSurface>
				<PreviewSurfaceMessage tone="danger">
					{t("structured_parse_failed")}
				</PreviewSurfaceMessage>
			</PreviewSurface>
		);
	}

	return (
		<PreviewSurface>
			<PreviewSurfaceToolbar
				icon="FileCode"
				label={t("preview_mode_xml")}
				meta={t("preview_mode_formatted")}
			/>
			<PreviewSurfaceContent>
				<ScrollArea className="h-full bg-background/80 dark:bg-background/25">
					<pre className="min-h-full p-4 font-mono text-sm whitespace-pre-wrap break-words">
						{formatted}
					</pre>
				</ScrollArea>
			</PreviewSurfaceContent>
		</PreviewSurface>
	);
}
