import { PreviewAppIcon } from "@/components/common/PreviewAppIcon";
import { Button } from "@/components/ui/button";
import { DialogHeader, DialogTitle } from "@/components/ui/dialog";
import { Icon } from "@/components/ui/icon";
import { cn } from "@/lib/utils";
import type { FileInfo, FileListItem } from "@/types/api";
import { AnimatedCollapsible } from "./AnimatedCollapsible";
import { FilePreviewFileSummary } from "./FilePreviewFileSummary";
import type { OpenWithMode, OpenWithOption } from "./types";

interface FilePreviewMethodChooserProps {
	file: FileInfo | FileListItem;
	activeMode: OpenWithMode | null;
	allOptions: OpenWithOption[];
	visibleOptions: OpenWithOption[];
	hiddenOptions: OpenWithOption[];
	showAllOpenMethods: boolean;
	thumbnailPath?: string;
	getOptionLabel: (option: OpenWithOption) => string;
	onClose: () => void;
	onSelect: (mode: OpenWithMode) => void;
	onShowAllOpenMethods: () => void;
	chooseOpenMethodLabel: string;
	closeLabel: string;
	moreOpenMethodsLabel: string;
}

export function FilePreviewMethodChooser({
	file,
	activeMode,
	allOptions,
	visibleOptions,
	hiddenOptions,
	showAllOpenMethods,
	thumbnailPath,
	getOptionLabel,
	onClose,
	onSelect,
	onShowAllOpenMethods,
	chooseOpenMethodLabel,
	closeLabel,
	moreOpenMethodsLabel,
}: FilePreviewMethodChooserProps) {
	return (
		<>
			<DialogHeader className="border-b bg-background px-5 py-4">
				<div className="flex items-start gap-3">
					<div className="min-w-0 flex-1 space-y-3">
						<DialogTitle className="truncate text-base font-semibold">
							{chooseOpenMethodLabel}
						</DialogTitle>
						<FilePreviewFileSummary file={file} thumbnailPath={thumbnailPath} />
					</div>
					<Button
						variant="ghost"
						size="icon-sm"
						onClick={onClose}
						aria-label={closeLabel}
						title={closeLabel}
					>
						<Icon name="X" className="size-4" />
						<span className="sr-only">{closeLabel}</span>
					</Button>
				</div>
			</DialogHeader>
			<div className="min-h-0 overflow-y-auto p-4">
				<div className="grid gap-2">
					{visibleOptions.map((option) => {
						const isActive = option.key === activeMode;
						return (
							<OpenMethodButton
								key={option.key}
								option={option}
								isActive={isActive}
								label={getOptionLabel(option)}
								onSelect={onSelect}
							/>
						);
					})}
					<AnimatedCollapsible open={showAllOpenMethods}>
						<div className="grid gap-2">
							{hiddenOptions.map((option) => {
								const isActive = option.key === activeMode;
								return (
									<OpenMethodButton
										key={option.key}
										option={option}
										isActive={isActive}
										label={getOptionLabel(option)}
										onSelect={onSelect}
									/>
								);
							})}
						</div>
					</AnimatedCollapsible>
					{!showAllOpenMethods && allOptions.length > 0 ? (
						<Button
							variant="ghost"
							className="h-11 justify-start rounded-lg border border-dashed px-3 text-left text-muted-foreground"
							onClick={onShowAllOpenMethods}
						>
							<div className="flex w-full items-center gap-2">
								<div className="min-w-0 flex-1">
									<div className="truncate text-sm font-medium">
										{moreOpenMethodsLabel}
									</div>
								</div>
								<Icon name="CaretDown" className="size-4" />
							</div>
						</Button>
					) : null}
				</div>
			</div>
		</>
	);
}

function OpenMethodButton({
	option,
	isActive,
	label,
	onSelect,
}: {
	option: OpenWithOption;
	isActive: boolean;
	label: string;
	onSelect: (mode: OpenWithMode) => void;
}) {
	return (
		<Button
			variant="ghost"
			className={cn(
				"h-14 justify-start rounded-lg border border-border/65 px-3 text-left hover:border-primary/35 hover:bg-muted/25",
				isActive &&
					"border-primary bg-accent text-foreground ring-1 ring-primary/20 hover:bg-accent",
			)}
			onClick={() => onSelect(option.key)}
		>
			<div className="flex w-full items-center gap-3">
				<div className="flex size-9 shrink-0 items-center justify-center rounded-md border border-border/50 bg-muted/35 text-muted-foreground">
					<PreviewAppIcon icon={option.icon} className="size-4" />
				</div>
				<div className="min-w-0 flex-1">
					<div className="truncate text-sm font-medium">{label}</div>
				</div>
				<Icon
					name={isActive ? "Check" : "CaretRight"}
					className="size-4 text-muted-foreground"
				/>
			</div>
		</Button>
	);
}
