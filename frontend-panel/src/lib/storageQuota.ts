export const BYTES_PER_MB = 1024 * 1024;
export const MAX_SAFE_STORAGE_QUOTA_MB = Math.floor(
	Number.MAX_SAFE_INTEGER / BYTES_PER_MB,
);

export function parseStorageQuotaMbToBytes(value: string) {
	const normalized = value.trim();
	if (!/^\d+$/.test(normalized)) {
		return null;
	}

	const mb = Number.parseInt(normalized, 10);
	if (
		!Number.isFinite(mb) ||
		!Number.isSafeInteger(mb) ||
		mb > MAX_SAFE_STORAGE_QUOTA_MB
	) {
		return null;
	}

	return mb * BYTES_PER_MB;
}
