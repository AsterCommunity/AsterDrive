import { STORAGE_KEYS } from "@/config/app";
import { logger } from "@/lib/logger";

function isUuid(value: string | null): value is string {
	return (
		typeof value === "string" &&
		/^[0-9a-f]{8}-[0-9a-f]{4}-[1-5][0-9a-f]{3}-[89ab][0-9a-f]{3}-[0-9a-f]{12}$/i.test(
			value,
		)
	);
}

function createUuid() {
	const browserCrypto = globalThis.crypto;
	if (typeof browserCrypto?.randomUUID === "function") {
		return browserCrypto.randomUUID();
	}
	if (typeof browserCrypto?.getRandomValues !== "function") {
		const randomHex = () =>
			Math.floor(Math.random() * 0xffff)
				.toString(16)
				.padStart(4, "0");
		return `${randomHex()}${randomHex()}-${randomHex()}-4${randomHex().slice(1)}-8${randomHex().slice(1)}-${randomHex()}${randomHex()}${randomHex()}`;
	}

	return "10000000-1000-4000-8000-100000000000".replace(/[018]/g, (char) =>
		(
			Number(char) ^
			(browserCrypto.getRandomValues(new Uint8Array(1))[0] &
				(15 >> (Number(char) / 4)))
		).toString(16),
	);
}

let memoryUploadFrontendClientId: string | null = null;

function getMemoryUploadFrontendClientId() {
	if (!isUuid(memoryUploadFrontendClientId)) {
		memoryUploadFrontendClientId = createUuid();
	}
	return memoryUploadFrontendClientId;
}

export function getUploadFrontendClientId(): string {
	try {
		const existing = localStorage.getItem(STORAGE_KEYS.uploadFrontendClientId);
		if (isUuid(existing)) return existing;

		const next = createUuid();
		localStorage.setItem(STORAGE_KEYS.uploadFrontendClientId, next);
		return next;
	} catch (error) {
		logger.warn("failed to persist upload frontend client id", error);
		return getMemoryUploadFrontendClientId();
	}
}
