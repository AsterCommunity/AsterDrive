export interface NormalizedObjectStorageConnectionFields {
	endpoint: string;
	bucket: string;
}

export function normalizeObjectStorageConnectionFields(
	endpoint: string,
	bucket: string,
): NormalizedObjectStorageConnectionFields {
	return {
		endpoint: endpoint.trim(),
		bucket: bucket.trim(),
	};
}
