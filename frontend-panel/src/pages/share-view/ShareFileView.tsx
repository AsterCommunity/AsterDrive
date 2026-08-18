import type { ReactNode } from "react";
import { useTranslation } from "react-i18next";
import { UserAvatarImage } from "@/components/common/UserAvatarImage";
import { FileThumbnail } from "@/components/files/FileThumbnail";
import { Button } from "@/components/ui/button";
import { Icon } from "@/components/ui/icon";
import { formatBytes } from "@/lib/format";
import { shareService } from "@/services/shareService";
import type { FileInfo, SharePublicInfo } from "@/types/api";
import { ShareMetaLine } from "./ShareMetaLine";
import { SharePageShell } from "./ShareViewShell";
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
			{/* D9 hero 落地页（定稿概念图 concept-sharefile-hero）：单文件分享的本质是
			    下载落地页，视觉重心全给文件本身，分享者信息沉底 */}
			<main className="flex min-h-0 flex-1 items-center justify-center overflow-auto">
				<div className="flex w-full max-w-xl flex-col items-center gap-6 px-4 py-10 text-center">
					{singleShareFile ? (
						<FileThumbnail
							file={singleShareFile}
							size="lg"
							thumbnailPath={shareService.thumbnailPath(token)}
							className="size-56 overflow-hidden rounded-xl sm:size-72"
							imageClassName="h-full w-full object-contain"
							iconClassName="size-16"
						/>
					) : (
						<div className="flex size-40 items-center justify-center rounded-xl bg-muted/30">
							<Icon name="File" className="size-16 text-muted-foreground" />
						</div>
					)}

					<div className="min-w-0">
						<h1 className="break-words text-2xl font-semibold leading-tight sm:text-3xl">
							{info.name}
						</h1>
						<ShareMetaLine items={fileMeta} className="mt-3 justify-center" />
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
						<Button size="lg" onClick={onDownload} className="w-full sm:w-auto">
							<Icon name="Download" className="mr-2 size-5" />
							{t("files:download")}
						</Button>
					</div>

					<div className="flex items-center gap-2 text-sm text-muted-foreground">
						<UserAvatarImage
							avatar={info.shared_by.avatar}
							name={info.shared_by.name}
							size="sm"
							className="size-5 rounded-full"
						/>
						<span className="truncate">{shareOwnerText}</span>
					</div>
				</div>
			</main>
			{previewElement}
		</SharePageShell>
	);
}
