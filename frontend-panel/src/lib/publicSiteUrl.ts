let publicSiteUrls: string[] = [];
let publicSiteUrlSet = new Set<string>();

export function normalizePublicSiteUrl(value: string | null | undefined) {
	const normalized = value?.trim();
	if (!normalized) return null;
	try {
		const resolved = new URL(normalized);
		const validOrigin =
			(resolved.protocol === "http:" || resolved.protocol === "https:") &&
			(resolved.pathname === "" || resolved.pathname === "/") &&
			resolved.search === "" &&
			resolved.hash === "" &&
			resolved.username === "" &&
			resolved.password === "";
		if (!validOrigin) {
			return null;
		}
		return resolved.origin;
	} catch {
		return null;
	}
}

export function normalizePublicSiteUrls(
	value: readonly string[] | null | undefined,
) {
	if (!value) return [];
	const origins: string[] = [];
	for (const candidate of value) {
		const origin = normalizePublicSiteUrl(candidate);
		if (!origin) {
			return [];
		}
		if (!origins.includes(origin)) {
			origins.push(origin);
		}
	}

	return origins;
}

export function setPublicSiteUrls(value: readonly string[] | null | undefined) {
	publicSiteUrls = normalizePublicSiteUrls(value);
	publicSiteUrlSet = new Set(publicSiteUrls);
	return publicSiteUrls[0] ?? null;
}

export function getPublicSiteUrl() {
	return publicSiteUrls[0] ?? null;
}

export function getPublicSiteUrls() {
	return publicSiteUrls;
}

export function publicSiteUrlMatches(value: string | null | undefined) {
	const origin = normalizePublicSiteUrl(value);
	return Boolean(origin && publicSiteUrlSet.has(origin));
}

export function absoluteAppUrl(path: string) {
	if (typeof window === "undefined") return path;
	return new URL(path, getPublicSiteUrl() ?? window.location.origin).toString();
}
