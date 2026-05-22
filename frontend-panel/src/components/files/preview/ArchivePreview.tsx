import { useMemo } from "react";
import { useTranslation } from "react-i18next";
import { isArchiveFilenameEncoding } from "@/lib/archiveFilenameEncoding";
import {
	ArchivePreviewContent,
	ArchivePreviewErrorState,
} from "./ArchivePreviewContent";
import type { ArchivePreviewProps } from "./archivePreviewTypes";
import {
	buildArchiveDirectoryEntries,
	buildArchiveVisibleEntries,
} from "./archivePreviewUtils";
import { PreviewLoadingState } from "./PreviewLoadingState";
import { PreviewUnavailable } from "./PreviewUnavailable";
import { useArchivePreviewState } from "./useArchivePreviewState";

export function ArchivePreview({ loadManifest }: ArchivePreviewProps) {
	const { t } = useTranslation("files");
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

	if (loading) {
		return (
			<PreviewLoadingState
				text={t(pending ? "archive_preview_generating" : "loading_preview")}
				className="h-full"
			/>
		);
	}

	if (error || !manifest) {
		return (
			<ArchivePreviewErrorState
				error={error}
				onRetry={() => dispatch({ type: "retryRequested" })}
			/>
		);
	}

	return (
		<ArchivePreviewContent
			manifest={manifest}
			query={query}
			currentFolder={currentFolder}
			filenameEncoding={filenameEncoding}
			visibleEntries={visibleEntries}
			onQueryChange={(query) => dispatch({ type: "queryChanged", query })}
			onCurrentFolderChange={(currentFolder) =>
				dispatch({ type: "currentFolderChanged", currentFolder })
			}
			onOpenDirectory={openArchiveDirectory}
			onFilenameEncodingChange={handleFilenameEncodingChange}
		/>
	);
}
