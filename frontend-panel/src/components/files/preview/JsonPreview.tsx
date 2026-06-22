import { Highlight, themes } from "prism-react-renderer";
import { useMemo } from "react";
import { useTranslation } from "react-i18next";
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
import { withScopedPrismClassName } from "./prismClassNames";

interface JsonPreviewProps {
	resource: ResourcePath;
}

export function JsonPreview({ resource }: JsonPreviewProps) {
	const { t } = useTranslation("files");
	const { content, loading, error, reload } = useTextContent(resource);

	const formatted = useMemo(() => {
		if (!content) return null;
		try {
			return JSON.stringify(JSON.parse(content), null, 2);
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
				icon="BracketsCurly"
				label={t("preview_mode_json")}
				meta={t("preview_mode_formatted")}
			/>
			<PreviewSurfaceContent>
				<ScrollArea className="h-full bg-background/80 dark:bg-background/25">
					<Highlight theme={themes.github} code={formatted} language="json">
						{({ className, style, tokens, getLineProps, getTokenProps }) => (
							<pre
								className={`${className} min-h-full p-4 font-mono text-sm leading-6 whitespace-pre-wrap break-words`}
								style={{ ...style, background: "transparent", margin: 0 }}
							>
								{tokens.map((line) => {
									const lineText = line.map((token) => token.content).join("");
									const lineKey = `line-${lineText}`;
									const lineProps = withScopedPrismClassName(
										getLineProps({ line, key: lineKey }),
									);
									return (
										<div key={lineKey} {...lineProps}>
											{line.map((token) => {
												const tokenKey = `${lineKey}-${token.types.join("-")}-${token.content}`;
												const tokenProps = withScopedPrismClassName(
													getTokenProps({
														key: tokenKey,
														token,
													}),
												);
												return <span key={tokenKey} {...tokenProps} />;
											})}
										</div>
									);
								})}
							</pre>
						)}
					</Highlight>
				</ScrollArea>
			</PreviewSurfaceContent>
		</PreviewSurface>
	);
}
