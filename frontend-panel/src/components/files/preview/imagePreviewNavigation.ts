import type { PublicThumbnailSupport } from "@/types/api";
import { getFileTypeInfo } from "./file-capabilities";
import type { PreviewableFileLike } from "./types";

type ImagePreviewNavigationFile = PreviewableFileLike & {
	file_category?: string | null;
};

export interface ImagePreviewNavigation<
	TFile extends ImagePreviewNavigationFile,
> {
	nextFile?: TFile;
	previousFile?: TFile;
}

export function isImagePreviewNavigationFile(
	file: ImagePreviewNavigationFile,
	thumbnailSupport?: PublicThumbnailSupport | null,
) {
	return (
		file.file_category === "image" ||
		getFileTypeInfo(file, thumbnailSupport).category === "image"
	);
}

export function getImagePreviewNavigation<
	TFile extends ImagePreviewNavigationFile,
>(
	files: TFile[],
	currentFile: ImagePreviewNavigationFile | null | undefined,
	thumbnailSupport?: PublicThumbnailSupport | null,
): ImagePreviewNavigation<TFile> {
	if (!currentFile) return {};

	const imageFiles = files.filter((file) =>
		isImagePreviewNavigationFile(file, thumbnailSupport),
	);
	if (imageFiles.length <= 1) return {};

	const currentIndex = imageFiles.findIndex(
		(file) => file.id === currentFile.id,
	);
	if (currentIndex < 0) return {};

	return {
		nextFile: imageFiles[(currentIndex + 1) % imageFiles.length],
		previousFile:
			imageFiles[(currentIndex - 1 + imageFiles.length) % imageFiles.length],
	};
}
