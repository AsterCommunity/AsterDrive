import { useTranslation } from "react-i18next";
import Markdown from "react-markdown";
import rehypeSanitize from "rehype-sanitize";
import remarkGfm from "remark-gfm";
import { ScrollArea } from "@/components/ui/scroll-area";
import { useTextContent } from "@/hooks/useTextContent";
import { PreviewError } from "./PreviewError";
import { PreviewLoadingState } from "./PreviewLoadingState";
import {
	PreviewSurface,
	PreviewSurfaceContent,
	PreviewSurfaceToolbar,
} from "./PreviewSurface";

interface MarkdownPreviewProps {
	path: string;
}

export function MarkdownPreview({ path }: MarkdownPreviewProps) {
	const { t } = useTranslation("files");
	const { content, loading, error, reload } = useTextContent(path);

	if (loading) {
		return (
			<PreviewLoadingState text={t("loading_preview")} className="h-full" />
		);
	}

	if (error || content === null) {
		return <PreviewError onRetry={() => void reload()} />;
	}

	return (
		<PreviewSurface>
			<PreviewSurfaceToolbar
				icon="FileText"
				label={t("preview_mode_markdown")}
				meta={t("preview_mode_rendered")}
			/>
			<PreviewSurfaceContent>
				<ScrollArea className="h-full bg-background/80 dark:bg-background/25">
					<div className="prose prose-sm dark:prose-invert max-w-none px-6 py-5">
						<Markdown
							remarkPlugins={[remarkGfm]}
							rehypePlugins={[rehypeSanitize]}
						>
							{content}
						</Markdown>
					</div>
				</ScrollArea>
			</PreviewSurfaceContent>
		</PreviewSurface>
	);
}
