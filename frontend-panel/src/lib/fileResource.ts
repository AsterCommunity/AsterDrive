import { shouldSendResourceCredentials } from "@/lib/apiUrl";
import type {
	FileResourceDeliveryMode,
	ReadyFileResourceHandle,
} from "@/lib/resourceRequest";
import type { PreviewLinkInfo } from "@/types/api";

const PREVIEW_LINK_EXPIRY_SKEW_MS = 10_000;

interface CachedPreviewLink {
	link: PreviewLinkInfo;
	expiresAtMs: number;
}

interface FileResourceOptions {
	cacheKey?: string;
	deliveryMode: FileResourceDeliveryMode;
	etag?: string | null;
	mimeType?: string;
	scope?: "personal" | "team" | "share";
}

const previewLinkCache = new Map<string, CachedPreviewLink>();

export function clearPreviewLinkResourceCache() {
	previewLinkCache.clear();
}

function previewLinkCacheKey(cacheKey: string, etag?: string | null) {
	return `${cacheKey}\u0000${etag ?? ""}`;
}

function previewLinkExpiresAtMs(link: PreviewLinkInfo) {
	const expiresAtMs = Date.parse(link.expires_at);
	return Number.isFinite(expiresAtMs) ? expiresAtMs : 0;
}

function isPreviewLinkUsable(entry: CachedPreviewLink, now = Date.now()) {
	return entry.expiresAtMs - PREVIEW_LINK_EXPIRY_SKEW_MS > now;
}

function withQueryParam(path: string, key: string, value: string) {
	const hashIndex = path.indexOf("#");
	const base = hashIndex >= 0 ? path.slice(0, hashIndex) : path;
	const hash = hashIndex >= 0 ? path.slice(hashIndex) : "";
	const queryIndex = base.indexOf("?");
	const pathname = queryIndex >= 0 ? base.slice(0, queryIndex) : base;
	const query = queryIndex >= 0 ? base.slice(queryIndex + 1) : "";
	const params = new URLSearchParams(query);
	params.set(key, value);
	const nextQuery = params.toString();
	return `${pathname}${nextQuery ? `?${nextQuery}` : ""}${hash}`;
}

function readyResource({
	cacheKey,
	conditionalHeaders,
	credentials,
	deliveryMode,
	etag,
	mimeType,
	redirectPolicy,
	requestPath,
	scope,
}: FileResourceOptions & {
	conditionalHeaders: "allowed" | "forbidden";
	credentials: RequestCredentials;
	redirectPolicy: "same_origin_only" | "may_cross_origin";
	requestPath: string;
}): ReadyFileResourceHandle {
	return {
		kind: "ready",
		identity: {
			cacheKey: cacheKey ?? requestPath,
			etag: etag ?? null,
			scope,
		},
		request: {
			url: requestPath,
			credentials,
			conditionalHeaders,
			redirectPolicy,
		},
		delivery: {
			mode: deliveryMode,
			mimeType,
		},
	};
}

export function authenticatedDownloadResource(
	downloadPath: string,
	options: FileResourceOptions,
) {
	return readyResource({
		...options,
		cacheKey: options.cacheKey ?? downloadPath,
		conditionalHeaders: "forbidden",
		credentials: "include",
		redirectPolicy: "may_cross_origin",
		requestPath: withQueryParam(downloadPath, "disposition", "inline"),
	});
}

export function derivedFileResource(
	path: string,
	options: FileResourceOptions,
) {
	// Derived endpoints are stable AsterDrive representations, not original file
	// storage access. Thumbnails, music cover thumbnails, backend image previews,
	// and avatars should be modeled with this local resource contract instead of
	// POSTing /resource-handle for every render. The resolver is reserved for
	// original content where storage policy, redirects, credentials, and delivery
	// mode need backend arbitration.
	return readyResource({
		...options,
		cacheKey: options.cacheKey ?? path,
		conditionalHeaders: "allowed",
		credentials: shouldSendResourceCredentials(path) ? "include" : "omit",
		redirectPolicy: "same_origin_only",
		requestPath: path,
	});
}

export function previewLinkResource(
	downloadPath: string,
	link: PreviewLinkInfo,
	options: Pick<FileResourceOptions, "deliveryMode" | "mimeType" | "scope">,
) {
	return {
		...readyResource({
			...options,
			cacheKey: downloadPath,
			conditionalHeaders: "forbidden",
			credentials: "omit",
			etag: link.etag,
			redirectPolicy: "may_cross_origin",
			requestPath: link.path,
		}),
		lifecycle: {
			expiresAt: link.expires_at,
			maxUses: link.max_uses,
		},
	} satisfies ReadyFileResourceHandle;
}

export function readCachedPreviewLinkResource(
	downloadPath: string,
	options: Pick<FileResourceOptions, "deliveryMode" | "mimeType" | "scope"> & {
		etag?: string | null;
	} = { deliveryMode: "blob_url" },
) {
	const exact = previewLinkCache.get(
		previewLinkCacheKey(downloadPath, options.etag),
	);
	if (exact && isPreviewLinkUsable(exact)) {
		return previewLinkResource(downloadPath, exact.link, options);
	}

	if (options.etag) return null;

	for (const [key, entry] of previewLinkCache.entries()) {
		if (!key.startsWith(`${downloadPath}\u0000`)) continue;
		if (isPreviewLinkUsable(entry)) {
			return previewLinkResource(downloadPath, entry.link, options);
		}
		previewLinkCache.delete(key);
	}

	return null;
}

export function cachePreviewLinkResource(
	downloadPath: string,
	link: PreviewLinkInfo,
) {
	const expiresAtMs = previewLinkExpiresAtMs(link);
	if (expiresAtMs <= 0) return;
	previewLinkCache.set(previewLinkCacheKey(downloadPath, link.etag), {
		link,
		expiresAtMs,
	});
}

export function fileResourceCacheKeysForMutation(paths: {
	download: string;
	imagePreview?: string;
	thumbnail?: string;
}) {
	return [paths.download, paths.thumbnail, paths.imagePreview].filter(
		(path): path is string => Boolean(path),
	);
}
