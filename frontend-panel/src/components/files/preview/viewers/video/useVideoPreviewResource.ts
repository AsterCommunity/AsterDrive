import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import { prepareAuthenticatedResource } from "@/lib/authenticatedResource";
import { logger } from "@/lib/logger";
import { type ResourcePath, resourceRequestPath } from "@/lib/resourceRequest";
import type { ShareStreamSessionInfo } from "@/types/api";

interface UseVideoPreviewResourceOptions {
	createMediaStreamSession?: () => Promise<ShareStreamSessionInfo>;
	fileName: string;
	resource: ResourcePath | null;
}

interface VideoPreviewResourceError {
	error: unknown;
	message: string;
}

export function useVideoPreviewResource({
	createMediaStreamSession,
	fileName,
	resource,
}: UseVideoPreviewResourceOptions) {
	const inputsRef = useRef({ createMediaStreamSession, resource });
	const [resourceVersion, setResourceVersion] = useState(0);
	const [retryCount, setRetryCount] = useState(0);
	const [resolvedResource, setResolvedResource] = useState<{
		key: string;
		path: string;
	} | null>(null);
	const [error, setError] = useState<VideoPreviewResourceError | null>(null);

	const requestPath = resource ? resourceRequestPath(resource) : null;
	const resourceMode = createMediaStreamSession ? "stream" : "direct";
	const resourceKey = `${requestPath}:${resourceMode}:${resourceVersion}:${retryCount}`;
	const resolvedPath =
		resolvedResource?.key === resourceKey ? resolvedResource.path : null;

	useEffect(() => {
		if (
			inputsRef.current.resource === resource &&
			inputsRef.current.createMediaStreamSession === createMediaStreamSession
		) {
			return;
		}
		inputsRef.current = { createMediaStreamSession, resource };
		setRetryCount(0);
		setResourceVersion((version) => version + 1);
	}, [createMediaStreamSession, resource]);

	useEffect(() => {
		setError(null);
		if (!resource || !requestPath) {
			setResolvedResource(null);
			return;
		}

		let cancelled = false;

		const resolveDirectPath = async () => {
			await prepareAuthenticatedResource(resource);
			return requestPath;
		};

		const resolveLink = createMediaStreamSession
			? async () => (await createMediaStreamSession()).path
			: resolveDirectPath;
		const failureMessage = createMediaStreamSession
			? "media stream session creation failed"
			: "media resource preparation failed";

		resolveLink()
			.then((nextPath) => {
				if (cancelled) return;
				setResolvedResource({ key: resourceKey, path: nextPath });
			})
			.catch((resolveError) => {
				if (cancelled) return;
				logger.warn(failureMessage, fileName, resolveError);
				setResolvedResource(null);
				setError({ error: resolveError, message: failureMessage });
			});

		return () => {
			cancelled = true;
		};
	}, [fileName, resource, requestPath, createMediaStreamSession, resourceKey]);

	const retry = useCallback(() => {
		setRetryCount((count) => count + 1);
	}, []);

	return useMemo(
		() => ({
			error,
			loading: Boolean(resource && requestPath && !resolvedPath && !error),
			resolvedPath,
			resourceKey,
			retry,
		}),
		[error, requestPath, resolvedPath, resource, resourceKey, retry],
	);
}
