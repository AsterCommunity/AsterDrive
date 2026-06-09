import { useTranslation } from "react-i18next";
import { Button } from "@/components/ui/button";
import { Icon } from "@/components/ui/icon";
import { PreviewSurfaceMessage } from "./PreviewSurface";

interface PreviewErrorProps {
	messageKey?: string;
	onRetry?: () => void;
}

export function PreviewError({
	messageKey = "preview_load_failed",
	onRetry,
}: PreviewErrorProps) {
	const { t } = useTranslation("files");
	return (
		<PreviewSurfaceMessage role="alert">
			<div className="flex flex-col items-center gap-3">
				<div className="flex size-11 items-center justify-center rounded-lg border border-border/60 bg-card text-muted-foreground shadow-xs dark:bg-muted/25 dark:shadow-none">
					<Icon name="Warning" className="size-6" />
				</div>
				<p>{t(messageKey)}</p>
				{onRetry ? (
					<Button variant="outline" size="sm" onClick={onRetry}>
						<Icon name="ArrowCounterClockwise" className="mr-2 size-4" />
						{t("preview_retry")}
					</Button>
				) : null}
			</div>
		</PreviewSurfaceMessage>
	);
}
