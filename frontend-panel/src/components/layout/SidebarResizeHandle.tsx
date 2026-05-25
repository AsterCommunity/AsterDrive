import { useTranslation } from "react-i18next";
import {
	USER_SIDEBAR_MAX_WIDTH_PX,
	USER_SIDEBAR_MIN_WIDTH_PX,
} from "@/lib/constants";
import { cn } from "@/lib/utils";
import type { SidebarResizeHandleProps } from "./sidebarTypes";

export function SidebarResizeHandle({
	onKeyDown,
	onPointerDown,
	resizing,
	width,
}: SidebarResizeHandleProps) {
	const { t } = useTranslation();

	return (
		<hr
			aria-label={t("resize_sidebar")}
			aria-orientation="vertical"
			aria-valuemin={USER_SIDEBAR_MIN_WIDTH_PX}
			aria-valuemax={USER_SIDEBAR_MAX_WIDTH_PX}
			aria-valuenow={width}
			tabIndex={0}
			className={cn(
				"absolute inset-y-0 -right-1 z-20 m-0 hidden h-auto w-2 cursor-col-resize touch-none border-0 bg-transparent transition-colors md:block focus-visible:bg-primary/20 focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring hover:bg-primary/15",
				resizing && "bg-primary/25",
			)}
			onPointerDown={onPointerDown}
			onKeyDown={onKeyDown}
		/>
	);
}
