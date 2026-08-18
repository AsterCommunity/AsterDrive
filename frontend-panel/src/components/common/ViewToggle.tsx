import { useTranslation } from "react-i18next";
import { Button } from "@/components/ui/button";
import { Icon } from "@/components/ui/icon";
import {
	Tooltip,
	TooltipContent,
	TooltipTrigger,
} from "@/components/ui/tooltip";
import { cn } from "@/lib/utils";
import type { ViewMode } from "@/stores/fileStore";

interface ViewToggleProps {
	value: ViewMode;
	onChange: (mode: ViewMode) => void;
}

export function ViewToggle({ value, onChange }: ViewToggleProps) {
	const { t } = useTranslation("files");
	return (
		<div className="flex rounded-lg bg-muted/35 p-0.5 dark:bg-muted/25">
			<Tooltip>
				<TooltipTrigger
					render={
						<Button
							variant="ghost"
							size="icon"
							className={cn(
								"size-7 rounded-r-sm sm:h-8 sm:w-8",
								value === "list" && "bg-accent",
							)}
							onClick={() => onChange("list")}
						/>
					}
				>
					<Icon name="ListBullets" className="size-4" />
				</TooltipTrigger>
				<TooltipContent>{t("list_view")}</TooltipContent>
			</Tooltip>
			<Tooltip>
				<TooltipTrigger
					render={
						<Button
							variant="ghost"
							size="icon"
							className={cn(
								"size-7 rounded-l-sm sm:h-8 sm:w-8",
								value === "grid" && "bg-accent",
							)}
							onClick={() => onChange("grid")}
						/>
					}
				>
					<Icon name="Grid" className="size-4" />
				</TooltipTrigger>
				<TooltipContent>{t("grid_view")}</TooltipContent>
			</Tooltip>
		</div>
	);
}
