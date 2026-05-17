import { useEffect, useState } from "react";
import { getFileTypeInfo } from "@/components/files/preview/file-capabilities";
import { Icon } from "@/components/ui/icon";
import {
	hasLanguageIcon,
	isIconMapLoaded,
	LanguageIcon,
	loadLanguageIcons,
} from "@/components/ui/language-icon";
import { cn } from "@/lib/utils";
import type { FileCategory } from "@/types/api";
import type { FileTypeInfo } from "./preview/types";

interface FileTypeIconProps {
	mimeType: string;
	fileName?: string;
	fileCategory?: FileCategory;
	className?: string;
}

const LANGUAGE_ICON_CATEGORIES = new Set<FileTypeInfo["category"]>([
	"csv",
	"json",
	"markdown",
	"text",
	"tsv",
	"xml",
]);

const CATEGORY_TYPE_INFO: Record<FileCategory, FileTypeInfo> = {
	image: { category: "image", icon: "FileImage", color: "text-sky-500" },
	video: { category: "video", icon: "FileVideo", color: "text-violet-500" },
	audio: { category: "audio", icon: "FileAudio", color: "text-pink-500" },
	document: { category: "document", icon: "FileText", color: "text-blue-500" },
	spreadsheet: {
		category: "spreadsheet",
		icon: "Table",
		color: "text-green-600",
	},
	presentation: {
		category: "presentation",
		icon: "Presentation",
		color: "text-orange-500",
	},
	archive: { category: "archive", icon: "FileZip", color: "text-yellow-600" },
	code: { category: "text", icon: "FileCode", color: "text-slate-500" },
	other: { category: "unknown", icon: "File", color: "text-muted-foreground" },
};

export function FileTypeIcon({
	mimeType,
	fileName,
	fileCategory,
	className,
}: FileTypeIconProps) {
	const name = fileName ?? "unknown";
	const [loaded, setLoaded] = useState(isIconMapLoaded);

	useEffect(() => {
		if (loaded) return;

		let cancelled = false;

		void loadLanguageIcons().then(() => {
			if (!cancelled) {
				setLoaded(true);
			}
		});

		return () => {
			cancelled = true;
		};
	}, [loaded]);

	const typeInfo =
		fileCategory == null
			? getFileTypeInfo({
					mime_type: mimeType,
					name,
				})
			: CATEGORY_TYPE_INFO[fileCategory];

	if (
		LANGUAGE_ICON_CATEGORIES.has(typeInfo.category) &&
		loaded &&
		hasLanguageIcon(name)
	) {
		return <LanguageIcon name={name} className={className} />;
	}

	const { icon, color } = typeInfo;
	return <Icon name={icon} className={cn(color, className)} />;
}
