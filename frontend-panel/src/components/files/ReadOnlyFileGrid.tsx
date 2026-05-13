import { useTranslation } from "react-i18next";
import { FileCard } from "@/components/files/FileCard";
import { Icon } from "@/components/ui/icon";
import type { FileListItem, FolderListItem } from "@/types/api";

interface ReadOnlyFileGridProps {
	folders: FolderListItem[];
	files: FileListItem[];
	onFolderClick?: (folder: FolderListItem) => void;
	onFileClick?: (file: FileListItem) => void;
	onFileDownload?: (file: FileListItem) => void;
	getThumbnailPath?: (file: FileListItem) => string;
}

const GRID_CLASSES =
	"grid grid-cols-2 gap-3 sm:grid-cols-3 md:grid-cols-4 lg:grid-cols-5 xl:grid-cols-6";

export function ReadOnlyFileGrid({
	folders,
	files,
	onFolderClick,
	onFileClick,
	onFileDownload,
	getThumbnailPath,
}: ReadOnlyFileGridProps) {
	const { t } = useTranslation("files");
	const hasBoth = folders.length > 0 && files.length > 0;

	return (
		<div className="space-y-4 p-4">
			{folders.length > 0 && (
				<div className="space-y-2">
					{hasBoth && (
						<h3 className="px-1 text-sm font-medium text-muted-foreground">
							{t("folders_section")}
						</h3>
					)}
					<div className={GRID_CLASSES}>
						{folders.map((folder) => (
							<FileCard
								key={`folder-${folder.id}`}
								item={folder}
								isFolder
								selected={false}
								onSelect={() => {}}
								onClick={() => onFolderClick?.(folder)}
								draggable={false}
								selectable={false}
							/>
						))}
					</div>
				</div>
			)}

			{files.length > 0 && (
				<div className="space-y-2">
					{hasBoth && (
						<h3 className="px-1 text-sm font-medium text-muted-foreground">
							{t("files_section")}
						</h3>
					)}
					<div className={GRID_CLASSES}>
						{files.map((file) => (
							<div key={`file-${file.id}`} className="group relative">
								<FileCard
									item={file}
									isFolder={false}
									selected={false}
									onSelect={() => {}}
									onClick={() => onFileClick?.(file)}
									draggable={false}
									selectable={false}
									thumbnailPath={getThumbnailPath?.(file)}
								/>
								{onFileDownload && (
									<button
										type="button"
										className="absolute bottom-2 right-2 hidden rounded-md border bg-background/90 p-1.5 text-muted-foreground shadow-sm transition hover:text-foreground group-hover:block"
										onClick={(e) => {
											e.stopPropagation();
											onFileDownload(file);
										}}
									>
										<Icon name="Download" className="h-4 w-4" />
										<span className="sr-only">{t("download")}</span>
									</button>
								)}
							</div>
						))}
					</div>
				</div>
			)}
		</div>
	);
}
