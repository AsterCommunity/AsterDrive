import type { FormEvent } from "react";
import { useState } from "react";
import { useTranslation } from "react-i18next";
import { toast } from "sonner";
import { Button } from "@/components/ui/button";
import {
	Dialog,
	DialogContent,
	DialogDescription,
	DialogFooter,
	DialogHeader,
	DialogTitle,
} from "@/components/ui/dialog";
import {
	DialogLifecycle,
	useDialogActivity,
} from "@/components/ui/dialog-lifecycle";
import { Icon } from "@/components/ui/icon";
import { Input } from "@/components/ui/input";
import { Label } from "@/components/ui/label";
import { handleApiError } from "@/hooks/useApiError";
import { fileService } from "@/services/fileService";

interface OfflineDownloadDialogProps {
	open: boolean;
	onOpenChange: (open: boolean) => void;
	targetFolderId: number | null;
	targetFolderName?: string | null;
}

export function OfflineDownloadDialog(props: OfflineDownloadDialogProps) {
	return (
		<DialogLifecycle
			open={props.open}
			identity={props.targetFolderId}
			target={props}
			onOpenChange={props.onOpenChange}
		>
			{({ target, active, onOpenChange }) => (
				<OfflineDownloadDialogSession
					{...target}
					open={active}
					onOpenChange={onOpenChange}
				/>
			)}
		</DialogLifecycle>
	);
}

function OfflineDownloadDialogSession({
	open,
	onOpenChange,
	targetFolderId,
	targetFolderName,
}: OfflineDownloadDialogProps) {
	const isCurrent = useDialogActivity();
	const { t } = useTranslation(["core", "tasks"]);
	const [url, setUrl] = useState("");
	const [filename, setFilename] = useState("");
	const [expectedSha256, setExpectedSha256] = useState("");
	const [submitting, setSubmitting] = useState(false);

	const targetName = targetFolderName?.trim() || t("tasks:summary_root_folder");

	const handleSubmit = async (event: FormEvent) => {
		event.preventDefault();
		const trimmedUrl = url.trim();
		if (!trimmedUrl) return;
		try {
			const parsedUrl = new URL(trimmedUrl);
			if (parsedUrl.protocol !== "http:" && parsedUrl.protocol !== "https:") {
				toast.error(t("tasks:offline_download_url_invalid"));
				return;
			}
		} catch {
			toast.error(t("tasks:offline_download_url_invalid"));
			return;
		}

		setSubmitting(true);
		try {
			const task = await fileService.createOfflineDownloadTask({
				url: trimmedUrl,
				filename: filename.trim() || null,
				expected_sha256: expectedSha256.trim() || null,
				target_folder_id: targetFolderId,
			});
			toast.success(t("tasks:task_created_success"), {
				description: task.display_name,
			});
			onOpenChange(false);
		} catch (error) {
			if (isCurrent()) handleApiError(error);
		} finally {
			if (isCurrent()) setSubmitting(false);
		}
	};

	return (
		<Dialog
			open={open}
			onOpenChange={(nextOpen) => {
				if (!submitting) {
					onOpenChange(nextOpen);
				}
			}}
		>
			<DialogContent keepMounted className="max-w-xl p-0 gap-0 overflow-hidden">
				<DialogHeader className="px-4 pt-4 pb-3">
					<div className="flex items-start gap-3">
						<span className="flex size-9 shrink-0 items-center justify-center rounded-lg bg-accent/60 text-accent-foreground">
							<Icon name="LinkSimple" className="size-4.5" />
						</span>
						<div className="min-w-0 space-y-1">
							<DialogTitle>
								{t("tasks:offline_download_dialog_title")}
							</DialogTitle>
							<DialogDescription>
								{t("tasks:offline_download_dialog_hint")}
							</DialogDescription>
						</div>
					</div>
				</DialogHeader>

				<form onSubmit={handleSubmit} className="space-y-4 px-4 pb-4">
					<div className="space-y-2">
						<Label htmlFor="offline-download-url">
							{t("tasks:offline_download_url_label")}
						</Label>
						<Input
							id="offline-download-url"
							type="url"
							value={url}
							onChange={(event) => setUrl(event.target.value)}
							placeholder={t("tasks:offline_download_url_placeholder")}
							disabled={submitting}
							spellCheck={false}
							autoFocus
							required
						/>
					</div>

					<div className="grid gap-3 sm:grid-cols-2">
						<div className="space-y-2">
							<Label htmlFor="offline-download-filename">
								{t("tasks:offline_download_filename_label")}
							</Label>
							<Input
								id="offline-download-filename"
								value={filename}
								onChange={(event) => setFilename(event.target.value)}
								placeholder={t("tasks:offline_download_filename_placeholder")}
								disabled={submitting}
							/>
						</div>
						<div className="space-y-2">
							<Label>{t("tasks:offline_download_target_folder_label")}</Label>
							<div className="flex h-9 min-w-0 items-center gap-2 rounded-md border bg-muted/25 px-3 text-sm">
								<Icon
									name={targetFolderId === null ? "House" : "FolderOpen"}
									className="size-4 shrink-0 text-muted-foreground"
								/>
								<span className="truncate">{targetName}</span>
							</div>
						</div>
					</div>

					<div className="space-y-2">
						<Label htmlFor="offline-download-sha256">
							{t("tasks:offline_download_sha256_label")}
						</Label>
						<Input
							id="offline-download-sha256"
							value={expectedSha256}
							onChange={(event) => setExpectedSha256(event.target.value)}
							placeholder={t("tasks:offline_download_sha256_placeholder")}
							disabled={submitting}
							spellCheck={false}
							className="font-mono text-xs"
						/>
					</div>

					<DialogFooter className="gap-2 sm:justify-end">
						<Button
							type="button"
							variant="outline"
							onClick={() => onOpenChange(false)}
							disabled={submitting}
						>
							{t("core:cancel")}
						</Button>
						<Button type="submit" disabled={submitting || !url.trim()}>
							<Icon
								name={submitting ? "Spinner" : "LinkSimple"}
								className={`size-4 ${submitting ? "animate-spin" : ""}`}
							/>
							{t("tasks:offline_download_submit")}
						</Button>
					</DialogFooter>
				</form>
			</DialogContent>
		</Dialog>
	);
}
