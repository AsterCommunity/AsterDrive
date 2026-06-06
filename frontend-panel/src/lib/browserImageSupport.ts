const BROWSER_RENDERABLE_IMAGE_EXTENSIONS = new Set([
	"avif",
	"bmp",
	"dib",
	"gif",
	"ico",
	"jpe",
	"jpeg",
	"jfif",
	"jpg",
	"png",
	"svg",
	"webp",
]);

const BROWSER_NON_RENDERABLE_IMAGE_EXTENSIONS = new Set([
	"3fr",
	"arw",
	"cr2",
	"cr3",
	"dng",
	"erf",
	"heic",
	"heif",
	"j2k",
	"jpf",
	"jp2",
	"jpx",
	"jxl",
	"kdc",
	"mrw",
	"nef",
	"nrw",
	"orf",
	"pef",
	"raf",
	"raw",
	"rw2",
	"srw",
	"tif",
	"tiff",
	"x3f",
]);

const BROWSER_RENDERABLE_IMAGE_MIME_TYPES = new Set([
	"image/avif",
	"image/bmp",
	"image/gif",
	"image/jpg",
	"image/jpeg",
	"image/pjpeg",
	"image/png",
	"image/svg+xml",
	"image/vnd.microsoft.icon",
	"image/webp",
	"image/x-icon",
	"image/x-ms-bmp",
	"image/x-png",
]);

interface BrowserImageFileLike {
	mime_type?: string | null;
	name?: string | null;
}

function getFileExtension(fileName: string | null | undefined) {
	const trimmed = fileName?.trim().toLowerCase() ?? "";
	const dot = trimmed.lastIndexOf(".");
	if (dot <= 0 || dot === trimmed.length - 1) {
		return "";
	}
	return trimmed.slice(dot + 1);
}

function normalizeMimeType(mimeType: string | null | undefined) {
	return mimeType?.trim().toLowerCase().split(";", 1)[0] ?? "";
}

export function canBrowserRenderImage(file: BrowserImageFileLike) {
	const extension = getFileExtension(file.name);
	if (BROWSER_NON_RENDERABLE_IMAGE_EXTENSIONS.has(extension)) return false;
	if (BROWSER_RENDERABLE_IMAGE_EXTENSIONS.has(extension)) return true;

	const mimeType = normalizeMimeType(file.mime_type);
	if (BROWSER_RENDERABLE_IMAGE_MIME_TYPES.has(mimeType)) return true;

	return false;
}
