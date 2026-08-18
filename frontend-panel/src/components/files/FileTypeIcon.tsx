import { useEffect, useState } from "react";
import { getFileTypeInfo } from "@/components/files/preview/capabilities/file-capabilities";
import type { FileCategory as PreviewFileCategory } from "@/components/files/preview/capabilities/types";
import { Icon } from "@/components/ui/icon";
import {
	hasLanguageIcon,
	isIconMapLoaded,
	LanguageIcon,
	loadLanguageIcons,
} from "@/components/ui/language-icon";
import { cn } from "@/lib/utils";
import type { FileCategory } from "@/types/api";
import type { FileTypeInfo } from "./preview/capabilities/types";

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
		color: "text-emerald-600",
	},
	presentation: {
		category: "presentation",
		icon: "Presentation",
		color: "text-orange-500",
	},
	archive: { category: "archive", icon: "FileZip", color: "text-amber-600" },
	code: { category: "text", icon: "FileCode", color: "text-teal-600" },
	other: { category: "unknown", icon: "File", color: "text-muted-foreground" },
};

type BadgeHue =
	| "sky"
	| "violet"
	| "pink"
	| "red"
	| "blue"
	| "emerald"
	| "orange"
	| "amber"
	| "teal"
	| "muted";

/** 静态枚举保证 Tailwind 类名可被构建期扫描，禁止运行时拼接。 */
const BADGE_TINT_CLASSES: Record<BadgeHue, string> = {
	sky: "bg-sky-500/10 dark:bg-sky-400/15",
	violet: "bg-violet-500/10 dark:bg-violet-400/15",
	pink: "bg-pink-500/10 dark:bg-pink-400/15",
	red: "bg-red-500/10 dark:bg-red-400/15",
	blue: "bg-blue-500/10 dark:bg-blue-400/15",
	emerald: "bg-emerald-500/10 dark:bg-emerald-400/15",
	orange: "bg-orange-500/10 dark:bg-orange-400/15",
	amber: "bg-amber-500/10 dark:bg-amber-400/15",
	teal: "bg-teal-500/10 dark:bg-teal-400/15",
	muted: "bg-muted/40 dark:bg-muted/25",
};

const API_CATEGORY_HUE: Record<FileCategory, BadgeHue> = {
	image: "sky",
	video: "violet",
	audio: "pink",
	document: "blue",
	spreadsheet: "emerald",
	presentation: "orange",
	archive: "amber",
	code: "teal",
	other: "muted",
};

const PREVIEW_CATEGORY_HUE: Record<PreviewFileCategory, BadgeHue> = {
	image: "sky",
	video: "violet",
	audio: "pink",
	pdf: "red",
	markdown: "blue",
	csv: "emerald",
	tsv: "emerald",
	json: "teal",
	xml: "teal",
	text: "blue",
	archive: "amber",
	document: "blue",
	spreadsheet: "emerald",
	presentation: "orange",
	unknown: "muted",
};

/**
 * 无缩略图文件的大尺寸 fallback 底色（网格媒体区徽章）。
 * 色相与 FileTypeIcon 的图标色同族，让"类型 → 颜色"成为系统而不是随机。
 */
export function getFileBadgeTint({
	mimeType,
	fileName,
	fileCategory,
}: {
	mimeType: string;
	fileName?: string;
	fileCategory?: FileCategory;
}): string {
	const hue =
		fileCategory != null
			? API_CATEGORY_HUE[fileCategory]
			: PREVIEW_CATEGORY_HUE[
					getFileTypeInfo({ mime_type: mimeType, name: fileName ?? "unknown" })
						.category
				];
	return BADGE_TINT_CLASSES[hue];
}

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
