import { config } from "@/config/app";

export function joinApiUrl(base: string, path: string) {
	const normalizedBase = base.replace(/\/+$/, "");
	const normalizedPath = path.startsWith("/") ? path : `/${path}`;
	return `${normalizedBase}${normalizedPath}`;
}

function isConfiguredApiUrl(path: string) {
	try {
		const resourceUrl = new URL(path);
		const baseUrl = /^https?:\/\//i.test(config.apiBaseUrl)
			? new URL(config.apiBaseUrl)
			: typeof window !== "undefined"
				? new URL(config.apiBaseUrl, window.location.origin)
				: null;
		if (!baseUrl || resourceUrl.origin !== baseUrl.origin) return false;

		const basePath = baseUrl.pathname.replace(/\/+$/, "") || "/";
		return (
			basePath === "/" ||
			resourceUrl.pathname === basePath ||
			resourceUrl.pathname.startsWith(`${basePath}/`)
		);
	} catch {
		return false;
	}
}

export function isExternalResourceUrl(path: string) {
	if (/^blob:/i.test(path)) return true;
	return /^https?:\/\//i.test(path) && !isConfiguredApiUrl(path);
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

export function shouldSendResourceCredentials(path: string) {
	return !isExternalResourceUrl(path) && !isPublicResourcePath(path);
}

export function isBrowserAddressableResourcePath(path: string) {
	return (
		/^https?:\/\//i.test(path) ||
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
