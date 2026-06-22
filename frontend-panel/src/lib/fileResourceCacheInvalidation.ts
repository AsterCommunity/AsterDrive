import { invalidateBlobUrl } from "@/hooks/useBlobUrl";
import { invalidateTextContent } from "@/hooks/useTextContent";
import { fileResourceCacheKeysForMutation } from "@/lib/fileResource";

export interface FileResourceMutationCachePaths {
	download: string;
	imagePreview?: string;
	thumbnail?: string;
}

export function invalidateFileResourceCachesForMutation(
	paths: FileResourceMutationCachePaths,
) {
	const [downloadKey, ...blobKeys] = fileResourceCacheKeysForMutation(paths);
	invalidateTextContent(downloadKey);
	for (const cacheKey of [downloadKey, ...blobKeys]) {
		invalidateBlobUrl(cacheKey);
	}
}

export function invalidateAllFileResourceCaches() {
	invalidateBlobUrl();
	invalidateTextContent();
}
