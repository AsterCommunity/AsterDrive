/**
 * An SVG is passive while rendered by <img>, but an application-origin Blob URL is also a
 * navigable document. The browser's image context restrictions disappear on that navigation.
 * Preserve the exact SVG bytes (including vector graphics/SMIL) using a data URL: image rendering
 * stays unchanged, while a standalone document has an opaque origin rather than Drive's origin.
 * This is only a representation URL; the raw Blob still owns downloads and authorized caching.
 * Configured embed/WOPI application URLs never enter this content representation layer.
 */
export function createBlobDisplayUrl(blob: Blob): Promise<string> {
	if (blob.type.split(";", 1)[0].trim().toLowerCase() !== "image/svg+xml") {
		return Promise.resolve(URL.createObjectURL(blob));
	}
	return new Promise((resolve, reject) => {
		const reader = new FileReader();
		reader.onload = () => {
			if (typeof reader.result === "string") resolve(reader.result);
			else reject(new Error("SVG display URL could not be read"));
		};
		reader.onerror = () =>
			reject(reader.error ?? new Error("SVG display URL read failed"));
		reader.onabort = () =>
			reject(new DOMException("SVG display URL read canceled", "AbortError"));
		reader.readAsDataURL(blob);
	});
}
