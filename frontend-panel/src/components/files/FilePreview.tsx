import { FilePreviewDialog } from "@/components/files/preview/dialog/FilePreviewDialog";
import type { FilePreviewResources } from "@/components/files/preview/resources/filePreviewResources";
import { DialogLifecycle } from "@/components/ui/dialog-lifecycle";
import type { FileInfo, FileListItem } from "@/types/api";

export interface FilePreviewImageNavigation {
	nextFile?: FileInfo | FileListItem;
	onNavigate: (file: FileInfo | FileListItem) => void;
	previousFile?: FileInfo | FileListItem;
}

interface FilePreviewProps {
	file: FileInfo | FileListItem | null;
	onClose: () => void;
	onFileUpdated?: () => void;
	editable?: boolean;
	resources: FilePreviewResources | null;
	imageNavigation?: FilePreviewImageNavigation;
	open?: boolean;
	openMode?: "auto" | "direct" | "picker";
}

export function FilePreview(props: FilePreviewProps) {
	const open =
		(props.open ?? true) && props.file !== null && props.resources !== null;
	return (
		<DialogLifecycle
			open={open}
			target={props}
			identity={props.file?.id}
			onOpenChange={(nextOpen) => {
				if (!nextOpen) props.onClose();
			}}
		>
			{({ target, active, onOpenChange }) =>
				target.file && target.resources ? (
					<FilePreviewDialog
						{...target}
						file={target.file}
						resources={target.resources}
						open={active}
						onClose={() => onOpenChange(false)}
					/>
				) : null
			}
		</DialogLifecycle>
	);
}
