import { type ReactNode, type Ref, useImperativeHandle } from "react";
import { useTranslation } from "react-i18next";
import { Icon } from "@/components/ui/icon";
import { cn } from "@/lib/utils";
import { useUploadAreaControlsStore } from "@/stores/uploadAreaControlsStore";

interface UploadAreaProps {
	children: ReactNode;
	ref?: Ref<UploadAreaHandle>;
}

export interface UploadAreaHandle {
	triggerFileUpload: () => void;
	triggerFolderUpload: () => void;
}

export function UploadArea({ children, ref }: UploadAreaProps) {
	const { t } = useTranslation(["core", "files"]);
	const controls = useUploadAreaControlsStore((state) => state.controls);

	useImperativeHandle(
		ref,
		() => ({
			triggerFileUpload: () => controls?.triggerFileUpload(),
			triggerFolderUpload: () => controls?.triggerFolderUpload(),
		}),
		[controls],
	);

	return (
		// biome-ignore lint/a11y/noStaticElementInteractions: drop zone
		<div
			className="relative flex min-h-0 flex-1 flex-col overflow-hidden"
			onDragEnter={controls?.handleDragEnter}
			onDragLeave={controls?.handleDragLeave}
			onDragOver={controls?.handleDragOver}
			onDrop={(event) => {
				void controls?.handleDrop(event);
			}}
		>
			{children}

			{controls?.isDragging && (
				<div
					className={cn(
						"absolute inset-0 z-(--z-fixed) flex flex-col items-center justify-center rounded-lg border-2 border-dashed border-primary bg-background/80 backdrop-blur-sm",
					)}
				>
					<Icon name="Upload" className="mb-3 size-10 text-primary" />
					<p className="text-lg font-medium text-primary">
						{t("files:drop_files_or_folders")}
					</p>
					<p className="mt-1 text-sm text-muted-foreground">
						{t("files:drop_files_or_folders_desc")}
					</p>
				</div>
			)}
		</div>
	);
}
