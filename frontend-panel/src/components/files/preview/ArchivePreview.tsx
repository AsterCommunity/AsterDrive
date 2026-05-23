import { useMemo } from "react";
import { isArchiveFilenameEncoding } from "@/lib/archiveFilenameEncoding";
import { ArchivePreviewContent } from "./ArchivePreviewContent";
import type { ArchivePreviewProps } from "./archivePreviewTypes";
import {
	buildArchiveDirectoryEntries,
	buildArchiveVisibleEntries,
} from "./archivePreviewUtils";
import { PreviewUnavailable } from "./PreviewUnavailable";
import { useArchivePreviewState } from "./useArchivePreviewState";

export function ArchivePreview({ loadManifest }: ArchivePreviewProps) {
	const [state, dispatch] = useArchivePreviewState(loadManifest);
	const {
		manifest,
		query,
		currentFolder,
		loading,
		pending,
		error,
		filenameEncoding,
	} = state;

	const directoryEntries = useMemo(() => {
		if (!manifest) return new Map();
		return buildArchiveDirectoryEntries(manifest.entries);
	}, [manifest]);

	const visibleEntries = useMemo(() => {
		if (!manifest) return [];
		return buildArchiveVisibleEntries(
			manifest,
			directoryEntries,
			query,
			currentFolder,
		);
	}, [currentFolder, directoryEntries, manifest, query]);

	const openArchiveDirectory = (path: string) => {
		dispatch({ type: "directoryOpened", path });
	};
	const handleFilenameEncodingChange = (value: string | null) => {
		if (!isArchiveFilenameEncoding(value)) return;
		dispatch({ type: "filenameEncodingChanged", filenameEncoding: value });
	};

	if (!loadManifest) {
		return <PreviewUnavailable />;
	}

	return (
		<ArchivePreviewContent
			manifest={manifest}
			query={query}
			currentFolder={currentFolder}
			filenameEncoding={filenameEncoding}
			visibleEntries={visibleEntries}
			loading={loading}
			pending={pending}
			error={error}
			onQueryChange={(query) => dispatch({ type: "queryChanged", query })}
			onCurrentFolderChange={(currentFolder) =>
				dispatch({ type: "currentFolderChanged", currentFolder })
			}
			onOpenDirectory={openArchiveDirectory}
			onFilenameEncodingChange={handleFilenameEncodingChange}
			onRetry={() => dispatch({ type: "retryRequested" })}
		/>
	);
}
