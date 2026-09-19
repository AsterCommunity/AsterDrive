import type { ReactNode } from "react";
import { cn } from "@/lib/utils";
import type { FolderIcon } from "@/types/api";
import { folderIconCatalog } from "./folderIconCatalog";

interface FolderIconRendererProps {
	icon?: FolderIcon;
	className?: string;
	defaultIcon: ReactNode;
}

export function FolderIconRenderer({
	icon = { kind: "default" },
	className,
	defaultIcon,
}: FolderIconRendererProps) {
	if (icon.kind === "default") {
		return <>{defaultIcon}</>;
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
