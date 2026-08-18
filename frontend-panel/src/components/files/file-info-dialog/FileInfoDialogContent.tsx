import { FileItemStatusIndicators } from "@/components/files/FileItemStatusIndicators";
import type { ThumbnailFileLike } from "@/components/files/FileThumbnail";
import { MediaThumbnail } from "@/components/files/MediaThumbnail";
import { Button } from "@/components/ui/button";
import { Icon } from "@/components/ui/icon";
import { cn } from "@/lib/utils";
import type { DetailRow } from "./types";

const EMPTY_METADATA_ROWS: DetailRow[] = [];

interface FileInfoDialogContentProps {
	currentLocked: boolean;
	isDesktop: boolean;
	isShared: boolean | null;
	metadataRows?: DetailRow[];
	metadataTitle?: string;
	overviewRows: DetailRow[];
	statusRows: DetailRow[];
	summaryLabel: string;
	summarySubtitle: string;
	tagsSection?: React.ReactNode;
	targetIcon:
		| {
				type: "file";
				file: ThumbnailFileLike;
		  }
		| {
				type: "folder";
		  };
	title: string;
	onClose: () => void;
	closeLabel: string;
	overviewTitle: string;
	statusTitle: string;
}

function Section({
	children,
	className,
	title,
}: {
	children: React.ReactNode;
	className?: string;
	title?: string;
}) {
	// D9 去框化：裸 section，标题直落背景，分区靠间距（对齐 SettingsSection 语言）
	return (
		<section className={cn("space-y-3", className)}>
			{title ? (
				<h3 className="text-[11px] font-semibold uppercase tracking-[0.16em] text-muted-foreground">
					{title}
				</h3>
			) : null}
			{children}
		</section>
	);
}

function DetailList({ rows }: { rows: DetailRow[] }) {
	// 行分隔 hairline 保留（真分隔），对齐 SettingsRow 的首尾收边
	return (
		<dl>
			{rows.map((row) => (
				<div
					key={row.label}
					className="flex items-start justify-between gap-4 border-b py-3 first:pt-0 last:border-b-0 last:pb-0"
				>
					<dt className="text-sm text-muted-foreground">{row.label}</dt>
					<dd
						className={cn(
							"max-w-[14rem] text-right text-sm text-foreground",
							row.monospace && "font-mono text-[13px]",
						)}
					>
						{row.value}
					</dd>
				</div>
			))}
		</dl>
	);
}

export function FileInfoDialogContent({
	closeLabel,
	currentLocked,
	isDesktop,
	isShared,
	metadataRows = EMPTY_METADATA_ROWS,
	metadataTitle,
	onClose,
	overviewRows,
	overviewTitle,
	statusRows,
	statusTitle,
	summaryLabel,
	summarySubtitle,
	tagsSection,
	targetIcon,
	title,
}: FileInfoDialogContentProps) {
	return (
		<div className="space-y-6 p-4">
			<Section>
				<div className="flex items-start gap-3">
					<div className="flex size-14 shrink-0 items-center justify-center rounded-2xl bg-muted/35 text-muted-foreground dark:bg-muted/20">
						{targetIcon.type === "file" ? (
							<MediaThumbnail
								file={targetIcon.file}
								size="lg"
								className="rounded-2xl"
								iconClassName="size-8"
								imageClassName="h-full w-full object-cover"
							/>
						) : (
							<Icon name="Folder" className="size-8 text-amber-500" />
						)}
					</div>
					<div className="min-w-0 flex-1 space-y-2">
						<div className="space-y-1">
							<p className="text-[11px] font-semibold uppercase tracking-[0.16em] text-muted-foreground">
								{summaryLabel}
							</p>
							{isDesktop ? (
								<h2 className="line-clamp-2 break-words text-lg font-semibold text-foreground">
									{title}
								</h2>
							) : (
								<p className="line-clamp-2 break-words text-lg font-semibold text-foreground">
									{title}
								</p>
							)}
							<p className="text-sm text-muted-foreground">{summarySubtitle}</p>
						</div>
						<FileItemStatusIndicators
							isLocked={currentLocked}
							isShared={isShared ?? false}
						/>
					</div>
					{isDesktop ? (
						<Button
							type="button"
							variant="ghost"
							size="icon-sm"
							onClick={onClose}
							aria-label={closeLabel}
						>
							<Icon name="X" className="size-4" />
						</Button>
					) : null}
				</div>
			</Section>

			<Section title={overviewTitle}>
				<DetailList rows={overviewRows} />
			</Section>

			{tagsSection}

			{metadataRows.length > 0 ? (
				<Section title={metadataTitle}>
					<DetailList rows={metadataRows} />
				</Section>
			) : null}

			<Section title={statusTitle}>
				<DetailList rows={statusRows} />
			</Section>
		</div>
	);
}
