import type { ReactNode } from "react";
import { Button } from "@/components/ui/button";
import { DialogHeader } from "@/components/ui/dialog";
import { Icon } from "@/components/ui/icon";
import { ScrollArea } from "@/components/ui/scroll-area";
import { cn } from "@/lib/utils";
import type { FileInfo, FileListItem } from "@/types/api";
import { FilePreviewFileSummary } from "./FilePreviewFileSummary";

interface FilePreviewPanelProps {
	file: FileInfo | FileListItem;
	body: ReactNode;
	allOptionsCount: number;
	usesInnerScroll: boolean;
	fillsViewportHeight: boolean;
	isExpanded: boolean;
	isDirty: boolean;
	thumbnailPath?: string;
	onChooseOpenMethod: () => void;
	onToggleExpand: () => void;
	onClose: () => void;
	chooseOpenMethodLabel: string;
	enterFullscreenLabel: string;
	exitFullscreenLabel: string;
	closeLabel: string;
}

export function FilePreviewPanel({
	file,
	body,
	allOptionsCount,
	usesInnerScroll,
	fillsViewportHeight,
	isExpanded,
	isDirty,
	thumbnailPath,
	onChooseOpenMethod,
	onToggleExpand,
	onClose,
	chooseOpenMethodLabel,
	enterFullscreenLabel,
	exitFullscreenLabel,
	closeLabel,
}: FilePreviewPanelProps) {
	const fullscreenLabel = isExpanded
		? exitFullscreenLabel
		: enterFullscreenLabel;

	return (
		<>
			<DialogHeader className="gap-0 border-b bg-card px-4 py-3">
				<div className="flex items-center gap-3">
					<div className="min-w-0 flex-1">
						<FilePreviewFileSummary
							file={file}
							fileNameAsDialogTitle
							iconClassName="size-9"
							thumbnailPath={thumbnailPath}
						/>
					</div>
					<div className="flex items-center gap-1">
						{allOptionsCount > 1 ? (
							<Button
								variant="ghost"
								size="icon-sm"
								onClick={onChooseOpenMethod}
								disabled={isDirty}
								aria-label={chooseOpenMethodLabel}
								title={chooseOpenMethodLabel}
							>
								<Icon name="ListBullets" className="size-4" />
								<span className="sr-only">{chooseOpenMethodLabel}</span>
							</Button>
						) : null}
						<Button
							variant="ghost"
							size="icon-sm"
							onClick={onToggleExpand}
							aria-label={fullscreenLabel}
							title={fullscreenLabel}
						>
							<Icon
								name={isExpanded ? "ArrowsInCardinal" : "ArrowsOutCardinal"}
								className="size-4"
							/>
							<span className="sr-only">{fullscreenLabel}</span>
						</Button>
						<Button
							variant="ghost"
							size="icon-sm"
							onClick={onClose}
							aria-label={closeLabel}
							title={closeLabel}
						>
							<Icon name="X" className="size-4" />
						</Button>
					</div>
				</div>
			</DialogHeader>
			{usesInnerScroll ? (
				<div
					className={cn(
						"w-full bg-background/70 p-3 dark:bg-background/25",
						(fillsViewportHeight || isExpanded) && "min-h-0 flex-1",
					)}
				>
					{body}
				</div>
			) : (
				<ScrollArea
					className={cn(
						"w-full bg-background/70 dark:bg-background/25",
						(fillsViewportHeight || isExpanded) && "min-h-0 flex-1",
					)}
				>
					<div
						className={cn(
							"w-full p-3",
							(fillsViewportHeight || isExpanded) && "h-full min-h-full",
						)}
					>
						{body}
					</div>
				</ScrollArea>
			)}
		</>
	);
}
