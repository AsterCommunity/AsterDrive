import { Fragment, useEffect, useReducer, useRef } from "react";
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
import { invalidateFileResourceCachesForMutation } from "@/lib/fileResourceCacheInvalidation";
import { formatBytes, formatDateTime } from "@/lib/format";
import { type FileVersionPage, fileService } from "@/services/fileService";
import type { FileVersion } from "@/types/api";

interface VersionHistoryDialogProps {
	open: boolean;
	onOpenChange: (open: boolean) => void;
	onOpenChangeComplete?: (open: boolean) => void;
	fileId: number;
	fileName: string;
	mimeType?: string;
	onRestored?: () => void;
}

type VersionInlineConfirm = {
	kind: "restore" | "delete";
	version: FileVersion;
};

interface VersionHistoryState {
	deletingVersionId: number | null;
	inlineConfirm: VersionInlineConfirm | null;
	loading: boolean;
	loadingMore: boolean;
	nextAfterSequence: number | null;
	restoringVersionId: number | null;
	versions: FileVersion[];
}

type VersionHistoryAction =
	| { type: "close" }
	| { type: "delete-end"; page?: FileVersionPage }
	| { type: "delete-start"; versionId: number }
	| { type: "load-end"; page?: FileVersionPage }
	| { type: "load-more-end"; page?: FileVersionPage }
	| { type: "load-more-start" }
	| { type: "load-start" }
	| { type: "mutation-refresh-start" }
	| { type: "restore-end"; page?: FileVersionPage }
	| { type: "restore-start"; versionId: number }
	| { type: "set-inline-confirm"; inlineConfirm: VersionInlineConfirm | null };

const VERSION_HISTORY_INITIAL_STATE: VersionHistoryState = {
	deletingVersionId: null,
	inlineConfirm: null,
	loading: false,
	loadingMore: false,
	nextAfterSequence: null,
	restoringVersionId: null,
	versions: [],
};

function versionHistoryReducer(
	state: VersionHistoryState,
	action: VersionHistoryAction,
): VersionHistoryState {
	switch (action.type) {
		case "close":
			return VERSION_HISTORY_INITIAL_STATE;
		case "delete-end":
			return {
				...state,
				deletingVersionId: null,
				inlineConfirm: null,
				nextAfterSequence: action.page
					? action.page.nextAfterSequence
					: state.nextAfterSequence,
				versions: action.page?.items ?? state.versions,
			};
		case "delete-start":
			return { ...state, deletingVersionId: action.versionId };
		case "load-end":
			return {
				...state,
				loading: false,
				nextAfterSequence: action.page
					? action.page.nextAfterSequence
					: state.nextAfterSequence,
				versions: action.page?.items ?? state.versions,
			};
		case "load-more-end":
			return {
				...state,
				loadingMore: false,
				nextAfterSequence: action.page
					? action.page.nextAfterSequence
					: state.nextAfterSequence,
				versions: action.page
					? [...state.versions, ...action.page.items]
					: state.versions,
			};
		case "load-more-start":
			return { ...state, loadingMore: true };
		case "load-start":
			return { ...state, loading: true };
		case "mutation-refresh-start":
			return { ...VERSION_HISTORY_INITIAL_STATE, loading: true };
		case "restore-end":
			return {
				...state,
				inlineConfirm: null,
				nextAfterSequence: action.page
					? action.page.nextAfterSequence
					: state.nextAfterSequence,
				restoringVersionId: null,
				versions: action.page?.items ?? state.versions,
			};
		case "restore-start":
			return { ...state, restoringVersionId: action.versionId };
		case "set-inline-confirm":
			return { ...state, inlineConfirm: action.inlineConfirm };
	}
}

export function VersionHistoryDialog({
	open,
	onOpenChange,
	onOpenChangeComplete,
	fileId,
	fileName,
	mimeType,
	onRestored,
}: VersionHistoryDialogProps) {
	const { t } = useTranslation(["files", "core"]);
	const [
		{
			deletingVersionId,
			inlineConfirm,
			loading,
			loadingMore,
			nextAfterSequence,
			restoringVersionId,
			versions,
		},
		dispatch,
	] = useReducer(versionHistoryReducer, VERSION_HISTORY_INITIAL_STATE);
	const requestGeneration = useRef(0);
	const currentRevision = versions.find((version) => version.current) ?? null;
	const historicalVersionCount = versions.filter(
		(version) => !version.current,
	).length;

	const loadMore = async () => {
		if (nextAfterSequence === null || loadingMore) return;
		const generation = requestGeneration.current;
		const targetFileId = fileId;
		try {
			dispatch({ type: "load-more-start" });
			const page = await fileService.listVersions(
				targetFileId,
				nextAfterSequence,
			);
			if (requestGeneration.current !== generation) return;
			dispatch({ type: "load-more-end", page });
		} catch (e) {
			if (requestGeneration.current !== generation) return;
			handleApiError(e);
			dispatch({ type: "load-more-end" });
		}
	};

	const handleRestore = async (versionId: number) => {
		const generation = requestGeneration.current;
		const targetFileId = fileId;
		try {
			dispatch({ type: "restore-start", versionId });
			await fileService.restoreVersion(targetFileId, versionId);
			invalidateFileResourceCachesForMutation({
				download: fileService.downloadPath(targetFileId),
				thumbnail: fileService.thumbnailPath(targetFileId),
				imagePreview: fileService.imagePreviewPath(targetFileId),
			});
			toast.success(t("version_restored"));
			onRestored?.();
			if (requestGeneration.current !== generation) return;
			dispatch({ type: "mutation-refresh-start" });
			try {
				const page = await fileService.listVersions(targetFileId);
				if (requestGeneration.current !== generation) return;
				dispatch({ type: "load-end", page });
			} catch (e) {
				if (requestGeneration.current !== generation) return;
				handleApiError(e);
				dispatch({ type: "load-end" });
			}
		} catch (e) {
			if (requestGeneration.current !== generation) return;
			handleApiError(e);
			dispatch({ type: "restore-end" });
		}
	};

	const handleDelete = async (versionId: number) => {
		const generation = requestGeneration.current;
		const targetFileId = fileId;
		try {
			dispatch({ type: "delete-start", versionId });
			await fileService.deleteVersion(targetFileId, versionId);
			toast.success(t("version_deleted"));
			if (requestGeneration.current !== generation) return;
			dispatch({ type: "mutation-refresh-start" });
			try {
				const page = await fileService.listVersions(targetFileId);
				if (requestGeneration.current !== generation) return;
				dispatch({ type: "load-end", page });
			} catch (e) {
				if (requestGeneration.current !== generation) return;
				handleApiError(e);
				dispatch({ type: "load-end" });
			}
		} catch (e) {
			if (requestGeneration.current !== generation) return;
			handleApiError(e);
			dispatch({ type: "delete-end" });
		}
	};

	const requestInlineConfirm = (
		kind: VersionInlineConfirm["kind"],
		version: FileVersion,
	) => {
		dispatch({
			type: "set-inline-confirm",
			inlineConfirm:
				inlineConfirm?.kind === kind && inlineConfirm.version.id === version.id
					? null
					: { kind, version },
		});
	};
	const handleOpenChange = (nextOpen: boolean) => {
		if (!nextOpen) {
			requestGeneration.current += 1;
			dispatch({ type: "close" });
		}
		onOpenChange(nextOpen);
	};

	useEffect(() => {
		const generation = ++requestGeneration.current;
		dispatch({ type: "close" });
		if (open) {
			dispatch({ type: "load-start" });
			void fileService
				.listVersions(fileId)
				.then((page) => {
					if (requestGeneration.current === generation) {
						dispatch({ type: "load-end", page });
					}
				})
				.catch((error: unknown) => {
					if (requestGeneration.current === generation) {
						handleApiError(error);
						dispatch({ type: "load-end" });
					}
				});
		}
		return () => {
			if (requestGeneration.current === generation) {
				requestGeneration.current += 1;
			}
		};
	}, [fileId, open]);

	return (
		<Dialog
			open={open}
			onOpenChange={handleOpenChange}
			onOpenChangeComplete={onOpenChangeComplete}
		>
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
							{currentRevision !== null ? (
								<div className="mt-1 font-mono text-xs text-muted-foreground">
									v{currentRevision.version}
								</div>
							) : null}
						</div>
						<div className="text-xs text-muted-foreground">
							{t("version_history_count", {
								count: historicalVersionCount,
							})}
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
											<div className="flex items-center gap-2">
												<span>v{v.version}</span>
												{v.current ? (
													<span className="font-sans text-xs text-muted-foreground">
														{t("version_current")}
													</span>
												) : null}
											</div>
										</TableCell>
										<TableCell className="text-sm">
											{formatBytes(v.size)}
										</TableCell>
										<TableCell className="text-muted-foreground text-xs">
											{formatDateTime(v.created_at)}
										</TableCell>
										<TableCell>
											<div className="flex gap-1">
												{v.current ? null : (
													<>
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
																loadingMore ||
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
																loadingMore ||
																restoringVersionId !== null ||
																deletingVersionId !== null
															}
															onClick={() => requestInlineConfirm("delete", v)}
														>
															<Icon
																name={
																	deletingVersionId === v.id
																		? "Spinner"
																		: "Trash"
																}
																className={`size-3.5 ${deletingVersionId === v.id ? "animate-spin" : ""}`}
															/>
														</Button>
													</>
												)}
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
																	loadingMore ||
																	restoringVersionId !== null ||
																	deletingVersionId !== null
																}
																onClick={() =>
																	dispatch({
																		type: "set-inline-confirm",
																		inlineConfirm: null,
																	})
																}
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
																	loadingMore ||
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
				{nextAfterSequence !== null ? (
					<div className="mt-4 flex justify-center">
						<Button
							variant="outline"
							size="sm"
							disabled={
								loadingMore ||
								restoringVersionId !== null ||
								deletingVersionId !== null
							}
							onClick={() => void loadMore()}
						>
							{loadingMore ? t("version_loading_more") : t("version_load_more")}
						</Button>
					</div>
				) : null}
			</DialogContent>
		</Dialog>
	);
}
