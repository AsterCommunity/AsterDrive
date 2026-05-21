import type { IconName } from "@/components/ui/icon";
import type { FileCategory as ApiFileCategory } from "@/types/api";

export type FileCategory =
	| "image"
	| "video"
	| "audio"
	| "pdf"
	| "markdown"
	| "csv"
	| "tsv"
	| "json"
	| "xml"
	| "text"
	| "archive"
	| "document"
	| "spreadsheet"
	| "presentation"
	| "unknown";

export type OpenWithMode = string;

export type PreviewProviderKind =
	| "image"
	| "video"
	| "audio"
	| "pdf"
	| "markdown"
	| "table"
	| "formatted"
	| "code"
	| "archive"
	| "wopi"
	| "url_template";

export interface OpenWithOption {
	key: string;
	mode: PreviewProviderKind;
	labelKey: string;
	label?: string;
	labels?: Record<string, string>;
	icon: string;
	config?: Record<string, unknown>;
}

export interface FileTypeInfo {
	category: FileCategory;
	icon: IconName;
	color: string;
}

export interface FilePreviewProfile {
	category: FileCategory;
	isBlobPreview: boolean;
	isTextBased: boolean;
	isEditableText: boolean;
	defaultMode: string | null;
	options: OpenWithOption[];
	allOptions?: OpenWithOption[];
}

export interface PreviewableFileLike {
	id?: number;
	file_category?: ApiFileCategory;
	name: string;
	mime_type: string;
	size?: number;
}
