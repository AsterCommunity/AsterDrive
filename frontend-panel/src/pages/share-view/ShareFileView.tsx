import type { ReactNode } from "react";
import { useTranslation } from "react-i18next";
import { UserAvatarImage } from "@/components/common/UserAvatarImage";
import { FileThumbnail } from "@/components/files/FileThumbnail";
import { Button } from "@/components/ui/button";
import { Icon } from "@/components/ui/icon";
import { formatBytes } from "@/lib/format";
import { shareService } from "@/services/shareService";
import type { FileInfo, SharePublicInfo } from "@/types/api";
import { ShareMetaLine, SharePageShell } from "./ShareViewShell";
import {
	getAccessSummary,
	getDownloadSummary,
	getExpirySummary,
} from "./shareViewSummaries";

interface ShareFileViewProps {
	info: SharePublicInfo;
	previewElement: ReactNode;
	shareOwnerText: string;
	singleShareFile: FileInfo | null;
	token: string;
	onDownload: () => void;
	onPreviewFile: (file: FileInfo) => void;
}

export function ShareFileView({
	info,
	onDownload,
	onPreviewFile,
	previewElement,
	shareOwnerText,
	singleShareFile,
	token,
}: ShareFileViewProps) {
	const { t } = useTranslation(["core", "share", "files", "errors"]);
	const downloadSummary = getDownloadSummary(info, t);
	const expirySummary = getExpirySummary(info, t);
	const fileMeta = [
		typeof info.size === "number" ? formatBytes(info.size) : null,
		info.mime_type,
		downloadSummary,
		expirySummary,
		getAccessSummary(info, t),
	].filter(Boolean);

	return (
		<SharePageShell>
			<main className="min-h-0 flex-1 overflow-auto">
				<div className="mx-auto w-full max-w-5xl p-4 sm:py-6 md:px-6">
					<article className="overflow-hidden rounded-lg border border-border/70 bg-card/80 shadow-lg shadow-black/5 dark:bg-card/55 dark:shadow-none">
						<div className="flex min-w-0 items-center gap-3 border-b border-border/65 px-4 py-3 sm:px-5">
							<UserAvatarImage
								avatar={info.shared_by.avatar}
								name={info.shared_by.name}
								size="sm"
								className="rounded-lg"
							/>
							<div className="min-w-0">
								<div className="truncate text-sm font-medium text-foreground">
									{shareOwnerText}
								</div>
							</div>
						</div>
						<div className="grid gap-5 p-4 sm:p-5 md:grid-cols-[14rem_minmax(0,1fr)]">
							<div className="flex h-52 w-full items-center justify-center overflow-hidden rounded-lg border border-border/65 bg-muted/25 sm:h-64 md:aspect-square md:h-auto">
								{singleShareFile ? (
									<FileThumbnail
										file={singleShareFile}
										size="lg"
										thumbnailPath={shareService.thumbnailPath(token)}
									/>
								) : (
									<Icon name="File" className="size-16 text-muted-foreground" />
								)}
							</div>
							<div className="flex min-w-0 flex-col justify-center gap-5">
								<div className="min-w-0">
									<h1 className="break-words text-2xl font-semibold leading-tight sm:text-3xl">
										{info.name}
									</h1>
									<ShareMetaLine items={fileMeta} className="mt-2" />
								</div>
								<div className="flex flex-col gap-2 sm:flex-row">
									{singleShareFile ? (
										<Button
											variant="outline"
											size="lg"
											onClick={() => onPreviewFile(singleShareFile)}
											className="w-full sm:w-auto"
										>
											<Icon name="Eye" className="mr-2 size-5" />
											{t("files:preview")}
										</Button>
									) : null}
									<Button
										size="lg"
										onClick={onDownload}
										className="w-full sm:w-auto"
									>
										<Icon name="Download" className="mr-2 size-5" />
										{t("files:download")}
									</Button>
								</div>
							</div>
						</div>
					</article>
				</div>
			</main>
			{previewElement}
		</SharePageShell>
	);
}
