import { Fragment, useCallback, useEffect, useState } from "react";
import { useTranslation } from "react-i18next";
import { toast } from "sonner";
import { InlineConfirm } from "@/components/common/ManagerDialogShell";
import { FileTypeIcon } from "@/components/files/FileTypeIcon";
import { Button } from "@/components/ui/button";
import {
	Dialog,
	DialogContent,
	DialogHeader,
	DialogTitle,
} from "@/components/ui/dialog";
import { Icon } from "@/components/ui/icon";
import {
	Table,
	TableBody,
	TableCell,
	TableHead,
	TableHeader,
	TableRow,
} from "@/components/ui/table";
import { handleApiError } from "@/hooks/useApiError";
import { invalidateBlobUrl } from "@/hooks/useBlobUrl";
import { invalidateTextContent } from "@/hooks/useTextContent";
import { formatBytes, formatDateTime } from "@/lib/format";
import { fileService } from "@/services/fileService";
import type { FileVersion } from "@/types/api";

interface VersionHistoryDialogProps {
	open: boolean;
	onOpenChange: (open: boolean) => void;
	fileId: number;
	fileName: string;
	mimeType?: string;
	onRestored?: () => void;
}

type VersionInlineConfirm = {
	kind: "restore" | "delete";
	version: FileVersion;
};

function getCurrentVersionNumber(versions: FileVersion[]) {
	return (
		versions.reduce(
			(maxVersion, version) => Math.max(maxVersion, version.version),
			0,
		) + 1
	);
}

export function VersionHistoryDialog({
	open,
	onOpenChange,
	fileId,
	fileName,
	mimeType,
	onRestored,
}: VersionHistoryDialogProps) {
	const { t } = useTranslation(["files", "core"]);
	const [versions, setVersions] = useState<FileVersion[]>([]);
	const [loading, setLoading] = useState(false);
	const [restoringVersionId, setRestoringVersionId] = useState<number | null>(
		null,
	);
	const [deletingVersionId, setDeletingVersionId] = useState<number | null>(
		null,
	);
	const [inlineConfirm, setInlineConfirm] =
		useState<VersionInlineConfirm | null>(null);
	const currentVersion = loading ? null : getCurrentVersionNumber(versions);

	const load = useCallback(async () => {
		try {
			setLoading(true);
			const data = await fileService.listVersions(fileId);
			setVersions(data);
		} catch (e) {
			handleApiError(e);
		} finally {
			setLoading(false);
		}
	}, [fileId]);

	const handleRestore = async (versionId: number) => {
		try {
			setRestoringVersionId(versionId);
			await fileService.restoreVersion(fileId, versionId);
			const downloadPath = fileService.downloadPath(fileId);
			invalidateTextContent(downloadPath);
			invalidateBlobUrl(downloadPath);
			invalidateBlobUrl(fileService.thumbnailPath(fileId));
			invalidateBlobUrl(fileService.imagePreviewPath(fileId));
			toast.success(t("version_restored"));
			onRestored?.();
			setInlineConfirm(null);
		} catch (e) {
			handleApiError(e);
		} finally {
			setRestoringVersionId(null);
		}
	};

	const handleDelete = async (versionId: number) => {
		try {
			setDeletingVersionId(versionId);
			await fileService.deleteVersion(fileId, versionId);
			const data = await fileService.listVersions(fileId);
			setVersions(data);
			toast.success(t("version_deleted"));
			setInlineConfirm(null);
		} catch (e) {
			handleApiError(e);
		} finally {
			setDeletingVersionId(null);
		}
	};

	const requestInlineConfirm = (
		kind: VersionInlineConfirm["kind"],
		version: FileVersion,
	) => {
		setInlineConfirm((current) =>
			current?.kind === kind && current.version.id === version.id
				? null
				: { kind, version },
		);
	};

	useEffect(() => {
		if (open) {
			load();
			return;
		}

		setVersions([]);
		setLoading(false);
		setRestoringVersionId(null);
		setDeletingVersionId(null);
		setInlineConfirm(null);
	}, [load, open]);

	return (
		<Dialog open={open} onOpenChange={onOpenChange}>
			<DialogContent keepMounted className="max-w-lg">
				<DialogHeader>
					<div className="flex items-start gap-3 pr-8">
						{mimeType ? (
							<FileTypeIcon
								mimeType={mimeType}
								fileName={fileName}
								className="mt-0.5 size-5 shrink-0"
							/>
						) : null}
						<div className="min-w-0">
							<DialogTitle>
								{t("version_history_title", { name: fileName })}
							</DialogTitle>
						</div>
					</div>
				</DialogHeader>
				<div className="mb-4 rounded-lg border bg-muted/20 p-3">
					<div className="flex items-center gap-3">
						<div className="min-w-0 flex-1">
							<div className="text-sm font-medium text-foreground">
								{t("version_current")}
							</div>
							{currentVersion !== null ? (
								<div className="mt-1 font-mono text-xs text-muted-foreground">
									v{currentVersion}
								</div>
							) : null}
						</div>
						<div className="text-xs text-muted-foreground">
							{t("version_history_count", { count: versions.length })}
						</div>
					</div>
				</div>
				{loading ? (
					<p className="text-muted-foreground text-sm py-4 text-center">
						{t("loading_preview")}
					</p>
				) : versions.length === 0 ? (
					<p className="text-muted-foreground text-sm py-4 text-center">
						{t("version_empty")}
					</p>
				) : (
					<Table>
						<TableHeader>
							<TableRow>
								<TableHead>{t("version_column")}</TableHead>
								<TableHead>{t("version_size")}</TableHead>
								<TableHead>{t("version_date")}</TableHead>
								<TableHead className="w-20">{t("version_actions")}</TableHead>
							</TableRow>
						</TableHeader>
						<TableBody>
							{versions.map((v) => (
								<Fragment key={v.id}>
									<TableRow key={v.id}>
										<TableCell className="font-mono text-sm">
											v{v.version}
										</TableCell>
										<TableCell className="text-sm">
											{formatBytes(v.size)}
										</TableCell>
										<TableCell className="text-muted-foreground text-xs">
											{formatDateTime(v.created_at)}
										</TableCell>
										<TableCell>
											<div className="flex gap-1">
												<Button
													variant="ghost"
													size="icon"
													className="size-7"
													title={
														restoringVersionId === v.id
															? t("version_restoring")
															: t("version_restore")
													}
													disabled={
														restoringVersionId !== null ||
														deletingVersionId !== null
													}
													onClick={() => requestInlineConfirm("restore", v)}
												>
													<Icon
														name={
															restoringVersionId === v.id
																? "Spinner"
																: "ArrowCounterClockwise"
														}
														className={`size-3.5 ${restoringVersionId === v.id ? "animate-spin" : ""}`}
													/>
												</Button>
												<Button
													variant="ghost"
													size="icon"
													className="size-7 text-destructive"
													title={
														deletingVersionId === v.id
															? t("version_deleting")
															: t("version_delete")
													}
													disabled={
														restoringVersionId !== null ||
														deletingVersionId !== null
													}
													onClick={() => requestInlineConfirm("delete", v)}
												>
													<Icon
														name={
															deletingVersionId === v.id ? "Spinner" : "Trash"
														}
														className={`size-3.5 ${deletingVersionId === v.id ? "animate-spin" : ""}`}
													/>
												</Button>
											</div>
										</TableCell>
									</TableRow>
									{inlineConfirm?.version.id === v.id ? (
										<TableRow key={`${v.id}-confirm`}>
											<TableCell colSpan={4} className="whitespace-normal">
												<InlineConfirm>
													<div className="flex flex-col gap-3 sm:flex-row sm:items-start sm:justify-between">
														<div className="space-y-1">
															<div className="text-sm font-medium">
																{inlineConfirm.kind === "restore"
																	? t("version_restore_confirm_title")
																	: t("version_delete_confirm_title")}
															</div>
															<p className="text-sm text-muted-foreground">
																{inlineConfirm.kind === "restore"
																	? t("version_restore_confirm_desc", {
																			version: v.version,
																		})
																	: t("version_delete_confirm_desc", {
																			version: v.version,
																		})}
															</p>
														</div>
														<div className="flex shrink-0 items-center gap-2 sm:justify-end">
															<Button
																variant="ghost"
																size="sm"
																disabled={
																	restoringVersionId !== null ||
																	deletingVersionId !== null
																}
																onClick={() => setInlineConfirm(null)}
															>
																{t("core:cancel")}
															</Button>
															<Button
																variant={
																	inlineConfirm.kind === "delete"
																		? "destructive"
																		: "default"
																}
																size="sm"
																disabled={
																	restoringVersionId !== null ||
																	deletingVersionId !== null
																}
																onClick={() => {
																	if (inlineConfirm.kind === "restore") {
																		void handleRestore(v.id);
																		return;
																	}
																	void handleDelete(v.id);
																}}
															>
																{inlineConfirm.kind === "restore"
																	? t("version_restore")
																	: t("version_delete")}
															</Button>
														</div>
													</div>
												</InlineConfirm>
											</TableCell>
										</TableRow>
									) : null}
								</Fragment>
							))}
						</TableBody>
					</Table>
				)}
			</DialogContent>
		</Dialog>
	);
}
