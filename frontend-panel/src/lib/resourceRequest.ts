export interface ResourceRequest {
	cacheKey?: string;
	etag?: string | null;
	credentials?: RequestCredentials;
	conditionalHeaders?: "allowed" | "forbidden";
	deliveryMode?: FileResourceDeliveryMode;
	redirectPolicy?: FileResourceRedirectPolicy;
	requestPath: string;
}

export type FileResourceDeliveryMode =
	| "blob_url"
	| "text"
	| "direct_url"
	| "media_stream"
	| "iframe_session"
	| "manifest";

export type FileResourceRedirectPolicy =
	| "same_origin_only"
	| "may_cross_origin";

export interface ReadyFileResourceHandle {
	kind: "ready";
	identity: {
		cacheKey: string;
		etag?: string | null;
		scope?: "personal" | "team" | "share";
	};
	request: {
		url: string;
		credentials: RequestCredentials;
		conditionalHeaders: "allowed" | "forbidden";
		redirectPolicy: FileResourceRedirectPolicy;
	};
	delivery: {
		mode: FileResourceDeliveryMode;
		mimeType?: string;
	};
	lifecycle?: {
		expiresAt?: string;
	};
}

export type FileResourceHandle =
	| { kind: "loading" }
	| { kind: "unavailable"; reason?: string }
	| ReadyFileResourceHandle;

export type ResourcePath = string | ResourceRequest | ReadyFileResourceHandle;

function isReadyFileResource(
	resource: ResourcePath,
): resource is ReadyFileResourceHandle {
	return (
		typeof resource === "object" &&
		"kind" in resource &&
		resource.kind === "ready"
	);
}

function isResourceRequest(
	resource: ResourcePath,
): resource is ResourceRequest {
	return typeof resource === "object" && !isReadyFileResource(resource);
}

export function resourceRequestPath(resource: ResourcePath) {
	if (isReadyFileResource(resource)) return resource.request.url;
	return typeof resource === "string" ? resource : resource.requestPath;
}

export function resourceCacheKey(resource: ResourcePath) {
	if (isReadyFileResource(resource)) return resource.identity.cacheKey;
	return typeof resource === "string"
		? resource
		: (resource.cacheKey ?? resource.requestPath);
}

export function resourceCanonicalEtag(resource: ResourcePath) {
	if (isReadyFileResource(resource)) return resource.identity.etag ?? null;
	return typeof resource === "string" ? null : (resource.etag ?? null);
}

export function resourceCredentials(
	resource: ResourcePath,
	fallback: (path: string) => boolean,
) {
	if (isReadyFileResource(resource)) {
		return resource.request.credentials === "include";
	}
	if (isResourceRequest(resource) && resource.credentials) {
		return resource.credentials === "include";
	}
	return fallback(resourceRequestPath(resource));
}

export function resourceConditionalHeaders(resource: ResourcePath) {
	if (isReadyFileResource(resource)) return resource.request.conditionalHeaders;
	if (isResourceRequest(resource) && resource.conditionalHeaders) {
		return resource.conditionalHeaders;
	}
	return "allowed";
}

export function resourceRedirectPolicy(resource: ResourcePath) {
	if (isReadyFileResource(resource)) return resource.request.redirectPolicy;
	if (isResourceRequest(resource) && resource.redirectPolicy) {
		return resource.redirectPolicy;
	}
	return "same_origin_only";
}

export function readyFileResourceFromRequest(
	resource: ResourceRequest,
	deliveryMode: FileResourceDeliveryMode,
): ReadyFileResourceHandle {
	return {
		kind: "ready",
		identity: {
			cacheKey: resource.cacheKey ?? resource.requestPath,
			etag: resource.etag ?? null,
		},
		request: {
			url: resource.requestPath,
			credentials: resource.credentials ?? "include",
			conditionalHeaders: resource.conditionalHeaders ?? "allowed",
			redirectPolicy: resource.redirectPolicy ?? "same_origin_only",
		},
		delivery: {
			mode: resource.deliveryMode ?? deliveryMode,
		},
	};
}
