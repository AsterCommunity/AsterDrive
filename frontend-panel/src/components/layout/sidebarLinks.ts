import type { IconName } from "@/components/ui/icon";
import type { FileCategory } from "@/types/api";

export const QUICK_CATEGORY_LINKS: Array<{
	category: FileCategory;
	icon: IconName;
	labelKey: string;
}> = [
	{ category: "image", icon: "FileImage", labelKey: "category_image" },
	{ category: "video", icon: "FileVideo", labelKey: "category_video" },
	{ category: "audio", icon: "FileAudio", labelKey: "category_audio" },
	{ category: "document", icon: "FileText", labelKey: "category_document" },
];
