import { useTranslation } from "react-i18next";
import { Button } from "@/components/ui/button";
import { Icon } from "@/components/ui/icon";
import {
	Tooltip,
	TooltipContent,
	TooltipTrigger,
} from "@/components/ui/tooltip";
import { DOWNLOAD_TASK_STATUS, useDownloadStore } from "@/stores/downloadStore";

export function DownloadCenterHeaderButton() {
	const { t } = useTranslation("files");
	const tasks = useDownloadStore((state) => state.tasks);
	const openPanel = useDownloadStore((state) => state.openPanel);
	const activeCount = tasks.filter(
		(task) =>
			task.status === DOWNLOAD_TASK_STATUS.queued ||
			task.status === DOWNLOAD_TASK_STATUS.preparing ||
			task.status === DOWNLOAD_TASK_STATUS.downloading,
	).length;

	if (tasks.length === 0) return null;

	return (
		<Tooltip>
			<TooltipTrigger
				render={
					<Button
						type="button"
						variant="ghost"
						size="icon-sm"
						className="relative rounded-full"
						onClick={openPanel}
						aria-label={t("download_center")}
					/>
				}
			>
				<Icon name="Download" className="size-4" />
				{activeCount > 0 ? (
					<span className="absolute -top-1 -right-1 flex min-w-4 items-center justify-center rounded-full bg-primary px-1 text-[10px] leading-4 text-primary-foreground">
						{activeCount}
					</span>
				) : null}
			</TooltipTrigger>
			<TooltipContent>{t("download_center")}</TooltipContent>
		</Tooltip>
	);
}
