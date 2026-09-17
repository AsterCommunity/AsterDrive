import { FcFolder } from "react-icons/fc";
import { cn } from "@/lib/utils";
import type { FolderIcon } from "@/types/api";
import { folderIconCatalog } from "./folderIconCatalog";

interface FolderIconRendererProps {
	icon?: FolderIcon;
	className?: string;
}

export function FolderIconRenderer({
	icon = { kind: "default" },
	className,
}: FolderIconRendererProps) {
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

	const Icon = icon.kind === "builtin" ? folderIconCatalog[icon.key] : FcFolder;
	return <Icon className={cn("shrink-0", className)} aria-hidden="true" />;
}
