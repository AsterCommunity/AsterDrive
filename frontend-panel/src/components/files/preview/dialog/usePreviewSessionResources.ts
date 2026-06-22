import { useCallback, useEffect, useMemo, useRef } from "react";
import type {
	ArchiveFilenameEncoding,
	ArchivePreviewManifest,
} from "@/types/api";
import type { OpenWithOption } from "../capabilities/types";
import type { FilePreviewResources } from "../resources/filePreviewResources";
import {
	createWopiSessionResource,
	type WopiSessionResource,
} from "../viewers/wopi/wopiSessionResource";

type ArchiveManifestLoader = (options?: {
	signal?: AbortSignal;
	filenameEncoding?: ArchiveFilenameEncoding;
}) => Promise<ArchivePreviewManifest>;

interface UsePreviewSessionResourcesOptions {
	activeOption: OpenWithOption | null;
	archiveManifestLoader?: ArchiveManifestLoader;
	fileId: number;
	launchWopiSession?: NonNullable<
		NonNullable<FilePreviewResources["actions"]>["launchWopiSession"]
	>;
	open: boolean;
}

export function usePreviewSessionResources({
	activeOption,
	archiveManifestLoader,
	fileId,
	launchWopiSession,
	open,
}: UsePreviewSessionResourcesOptions) {
	const archiveManifestLoaderRef = useRef(archiveManifestLoader);
	const wopiResourceRef = useRef<{
		key: string;
		launcher: typeof launchWopiSession;
		resource: WopiSessionResource;
	} | null>(null);

	useEffect(() => {
		archiveManifestLoaderRef.current = archiveManifestLoader;
	}, [archiveManifestLoader]);

	const launchActiveWopiSession = useCallback(() => {
		if (activeOption?.mode !== "wopi" || !launchWopiSession) {
			return Promise.reject(new Error("wopi session launcher unavailable"));
		}

		return launchWopiSession(activeOption.key);
	}, [activeOption, launchWopiSession]);

	const wopiSessionResource = useMemo(() => {
		if (activeOption?.mode !== "wopi" || !launchWopiSession) {
			return null;
		}

		const resourceKey = `${fileId}:${activeOption.key}`;
		if (
			wopiResourceRef.current?.key === resourceKey &&
			wopiResourceRef.current.launcher === launchWopiSession
		) {
			return wopiResourceRef.current.resource;
		}

		const resource = createWopiSessionResource(() =>
			launchWopiSession(activeOption.key),
		);
		wopiResourceRef.current = {
			key: resourceKey,
			launcher: launchWopiSession,
			resource,
		};
		return resource;
	}, [activeOption, fileId, launchWopiSession]);

	const stableArchiveManifestLoader = useCallback(
		(options?: Parameters<ArchiveManifestLoader>[0]) => {
			const loadManifest = archiveManifestLoaderRef.current;
			if (!loadManifest) {
				return Promise.reject(new Error("archive manifest loader unavailable"));
			}

			return loadManifest(options);
		},
		[],
	);

	return {
		activeArchiveManifestLoader:
			open && activeOption?.mode === "archive" && archiveManifestLoader
				? stableArchiveManifestLoader
				: undefined,
		launchWopiSession: launchWopiSession ? launchActiveWopiSession : null,
		wopiSessionResource,
	};
}
