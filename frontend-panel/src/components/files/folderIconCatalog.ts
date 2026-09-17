import type { IconType } from "react-icons";
import {
	FcBriefcase,
	FcCalendar,
	FcDatabase,
	FcDocument,
	FcHome,
	FcIdea,
	FcImageFile,
	FcLibrary,
	FcMusic,
	FcPackage,
	FcVideoFile,
} from "react-icons/fc";
import type { FolderBuiltinIcon } from "@/types/api";

export const folderIconCatalog = {
	documents: FcDocument,
	images: FcImageFile,
	music: FcMusic,
	videos: FcVideoFile,
	work: FcBriefcase,
	home: FcHome,
	archive: FcPackage,
	library: FcLibrary,
	database: FcDatabase,
	calendar: FcCalendar,
	ideas: FcIdea,
} satisfies Record<FolderBuiltinIcon, IconType>;

export const folderBuiltinIconKeys = Object.keys(
	folderIconCatalog,
) as FolderBuiltinIcon[];
