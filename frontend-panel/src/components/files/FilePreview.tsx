import { FilePreviewDialog } from "@/components/files/preview/FilePreviewDialog";
import type { MusicPlayerTrack } from "@/stores/musicPlayerStore";
import type {
	ArchiveFilenameEncoding,
	ArchivePreviewManifest,
	FileInfo,
	FileListItem,
	PreviewLinkInfo,
	ShareStreamSessionInfo,
	WopiLaunchSession,
} from "@/types/api";

export interface FilePreviewImageNavigation {
	nextFile?: FileInfo | FileListItem;
	onNavigate: (file: FileInfo | FileListItem) => void;
	previousFile?: FileInfo | FileListItem;
}

interface FilePreviewProps {
	file: FileInfo | FileListItem;
	onClose: () => void;
	onOpenChangeComplete?: (open: boolean) => void;
	onFileUpdated?: () => void;
	downloadPath?: string;
	imagePreviewPath?: string;
	thumbnailPath?: string;
	editable?: boolean;
	previewLinkFactory?: () => Promise<PreviewLinkInfo>;
	archivePreviewFactory?: (options?: {
		signal?: AbortSignal;
		filenameEncoding?: ArchiveFilenameEncoding;
	}) => Promise<ArchivePreviewManifest>;
	loadMusicBackendMetadata?: MusicPlayerTrack["loadBackendMetadata"];
	mediaStreamLinkFactory?: () => Promise<ShareStreamSessionInfo>;
	wopiSessionFactory?: (appKey: string) => Promise<WopiLaunchSession>;
	imageNavigation?: FilePreviewImageNavigation;
	open?: boolean;
	openMode?: "auto" | "direct" | "picker";
}

export function FilePreview({
	file,
	onClose,
	onOpenChangeComplete,
	onFileUpdated,
	downloadPath,
	imagePreviewPath,
	thumbnailPath,
	editable,
	previewLinkFactory,
	archivePreviewFactory,
	loadMusicBackendMetadata,
	mediaStreamLinkFactory,
	wopiSessionFactory,
	imageNavigation,
	open = true,
	openMode,
}: FilePreviewProps) {
	return (
		<FilePreviewDialog
			open={open}
			file={file}
			onClose={onClose}
			onOpenChangeComplete={onOpenChangeComplete}
			onFileUpdated={onFileUpdated}
			downloadPath={downloadPath}
			imagePreviewPath={imagePreviewPath}
			thumbnailPath={thumbnailPath}
			editable={editable}
			previewLinkFactory={previewLinkFactory}
			archivePreviewFactory={archivePreviewFactory}
			loadMusicBackendMetadata={loadMusicBackendMetadata}
			mediaStreamLinkFactory={mediaStreamLinkFactory}
			wopiSessionFactory={wopiSessionFactory}
			imageNavigation={imageNavigation}
			openMode={openMode}
		/>
	);
}
