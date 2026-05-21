export function getThumbnailExtension(fileName: string) {
	const trimmed = fileName.trim().toLowerCase();
	const dot = trimmed.lastIndexOf(".");
	if (dot <= 0 || dot === trimmed.length - 1) {
		return "";
	}
	return trimmed.slice(dot + 1);
}

export function supportsThumbnailExtension(
	fileName: string,
	extensions: string[] | undefined,
) {
	const extension = getThumbnailExtension(fileName);
	if (!extension || !extensions?.length) {
		return false;
	}

	return extensions.some(
		(candidate) =>
			candidate.trim().replace(/^\./, "").toLowerCase() === extension,
	);
}
