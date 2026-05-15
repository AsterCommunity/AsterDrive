import { config } from "@/config/app";

export function joinApiUrl(base: string, path: string) {
	const normalizedBase = base.replace(/\/+$/, "");
	const normalizedPath = path.startsWith("/") ? path : `/${path}`;
	return `${normalizedBase}${normalizedPath}`;
}

export function resolveApiResourceUrl(path: string) {
	if (/^https?:\/\//i.test(path) || path.startsWith("blob:")) {
		return path;
	}
	if (
		path.startsWith("/api/") ||
		path.startsWith("/d/") ||
		path.startsWith("/pv/")
	) {
		return path;
	}
	return joinApiUrl(config.apiBaseUrl, path);
}
