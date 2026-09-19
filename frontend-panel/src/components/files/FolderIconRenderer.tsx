import { cloneElement, type ReactElement } from "react";
import { cn } from "@/lib/utils";
import type { FolderIcon } from "@/types/api";
import { folderIconCatalog } from "./folderIconCatalog";

interface FolderIconRendererProps {
	icon?: FolderIcon;
	className?: string;
	defaultIcon: ReactElement;
}

export function FolderIconRenderer({
	icon = { kind: "default" },
	className,
	defaultIcon,
}: FolderIconRendererProps) {
	if (icon.kind === "default") {
		const defaultElement = defaultIcon as ReactElement<{
			"aria-hidden"?: boolean;
			focusable?: boolean;
			tabIndex?: number;
		}>;
		return cloneElement(defaultElement, {
			"aria-hidden": true,
			focusable: false,
			tabIndex: -1,
		});
	}

	if (icon.kind === "emoji") {
		return (
			<span
				className={cn(
					"inline-flex items-center justify-center font-sans leading-none",
					className,
				)}
				aria-hidden="true"
			>
				{icon.value}
			</span>
		);
	}

	const Icon = folderIconCatalog[icon.key];
	return <Icon className={cn("shrink-0", className)} aria-hidden="true" />;
}
