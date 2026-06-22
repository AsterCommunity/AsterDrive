import { FilePreviewDialog } from "@/components/files/preview/FilePreviewDialog";
import type { FilePreviewResources } from "@/components/files/preview/filePreviewResources";
import type { FileInfo, FileListItem } from "@/types/api";

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
	editable?: boolean;
	resources: FilePreviewResources;
	imageNavigation?: FilePreviewImageNavigation;
	open?: boolean;
	openMode?: "auto" | "direct" | "picker";
}

export function FilePreview({
	file,
	onClose,
	onOpenChangeComplete,
	onFileUpdated,
	editable,
	resources,
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
			editable={editable}
			resources={resources}
			imageNavigation={imageNavigation}
			openMode={openMode}
		/>
	);
}
