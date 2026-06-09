import { config } from "@/config/app";

export function joinApiUrl(base: string, path: string) {
	const normalizedBase = base.replace(/\/+$/, "");
	const normalizedPath = path.startsWith("/") ? path : `/${path}`;
	return `${normalizedBase}${normalizedPath}`;
}

export function isExternalResourceUrl(path: string) {
	return /^(?:https?:\/\/|blob:)/i.test(path);
}

export function normalizeApiResourcePath(path: string) {
	return path.replace(/^\/api\/v\d+(?=\/|$)/, "");
}

export function isPublicResourcePath(path: string) {
	const normalizedPath = normalizeApiResourcePath(path);
	return (
		normalizedPath.startsWith("/d/") ||
		normalizedPath.startsWith("/pv/") ||
		normalizedPath.startsWith("/s/") ||
		normalizedPath.startsWith("/public/")
	);
}

export function isBrowserAddressableResourcePath(path: string) {
	return (
		isExternalResourceUrl(path) ||
		path.startsWith("/api/") ||
		path.startsWith("/d/") ||
		path.startsWith("/pv/")
	);
}

export function resolveApiResourceUrl(path: string) {
	if (isBrowserAddressableResourcePath(path)) {
		return path;
	}
	return joinApiUrl(config.apiBaseUrl, path);
}
