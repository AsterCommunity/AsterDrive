import { useTranslation } from "react-i18next";
import { Button } from "@/components/ui/button";
import { Icon } from "@/components/ui/icon";
import { cn } from "@/lib/utils";
import { PreviewSurfaceMessage } from "./PreviewSurface";

interface PreviewErrorProps {
	appearance?: "default" | "dark";
	className?: string;
	messageKey?: string;
	onRetry?: () => void;
}

export function PreviewError({
	appearance = "default",
	className,
	messageKey = "preview_load_failed",
	onRetry,
}: PreviewErrorProps) {
	const { t } = useTranslation("files");
	const isDarkAppearance = appearance === "dark";

	return (
		<PreviewSurfaceMessage
			role="alert"
			className={cn(
				"h-full min-h-[12rem]",
				isDarkAppearance && "bg-zinc-950 text-zinc-400",
				className,
			)}
		>
			<div className="flex flex-col items-center gap-3">
				<div
					className={cn(
						"flex size-11 items-center justify-center rounded-lg border shadow-xs dark:shadow-none",
						isDarkAppearance
							? "border-white/16 bg-white/12 text-zinc-300 shadow-none"
							: "border-border/60 bg-card text-muted-foreground dark:bg-muted/25",
					)}
				>
					<Icon name="Warning" className="size-6" />
				</div>
				<p>{t(messageKey)}</p>
				{onRetry ? (
					<Button
						variant="outline"
						size="sm"
						className={
							isDarkAppearance
								? "border-white/14 bg-white/10 text-zinc-100 shadow-none hover:border-white/22 hover:bg-white/16 hover:text-white focus-visible:border-white/28 focus-visible:ring-white/18 dark:border-white/14 dark:bg-white/10 dark:hover:bg-white/16"
								: undefined
						}
						onClick={onRetry}
					>
						<Icon name="ArrowCounterClockwise" className="mr-2 size-4" />
						{t("preview_retry")}
					</Button>
				) : null}
			</div>
		</PreviewSurfaceMessage>
	);
}
