import { FileItemStatusIndicators } from "@/components/files/FileItemStatusIndicators";
import type { ThumbnailFileLike } from "@/components/files/FileThumbnail";
import { MediaThumbnail } from "@/components/files/MediaThumbnail";
import { Button } from "@/components/ui/button";
import { Icon } from "@/components/ui/icon";
import { cn } from "@/lib/utils";
import type { DetailRow } from "./types";

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
	return (
		<section
			className={cn(
				"space-y-3 rounded-2xl border border-border/60 bg-card/55 p-4 dark:bg-background/18",
				className,
			)}
		>
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
	return (
		<dl className="space-y-3">
			{rows.map((row) => (
				<div
					key={row.label}
					className="flex items-start justify-between gap-4 border-b border-border/40 pb-3 last:border-b-0 last:pb-0"
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
	metadataRows = [],
	metadataTitle,
	onClose,
	overviewRows,
	overviewTitle,
	statusRows,
	statusTitle,
	summaryLabel,
	summarySubtitle,
	targetIcon,
	title,
}: FileInfoDialogContentProps) {
	return (
		<div className="space-y-4 p-4">
			<Section className="gap-0 space-y-4 bg-card/55 dark:bg-background/18">
				<div className="flex items-start gap-3">
					<div className="flex h-14 w-14 shrink-0 items-center justify-center rounded-2xl bg-muted/35 text-muted-foreground dark:bg-muted/20">
						{targetIcon.type === "file" ? (
							<MediaThumbnail
								file={targetIcon.file}
								size="lg"
								className="rounded-2xl"
								iconClassName="h-8 w-8"
								imageClassName="h-full w-full object-cover"
							/>
						) : (
							<Icon name="Folder" className="h-8 w-8 text-amber-500" />
						)}
					</div>
					<div className="min-w-0 flex-1 space-y-2">
						<div className="space-y-1">
							<p className="text-[11px] font-semibold uppercase tracking-[0.16em] text-muted-foreground">
								{summaryLabel}
							</p>
							{isDesktop ? (
								<h2 className="line-clamp-2 text-lg font-semibold text-foreground">
									{title}
								</h2>
							) : (
								<p className="line-clamp-2 text-lg font-semibold text-foreground">
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
							<Icon name="X" className="h-4 w-4" />
						</Button>
					) : null}
				</div>
			</Section>

			<Section title={overviewTitle}>
				<DetailList rows={overviewRows} />
			</Section>

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
