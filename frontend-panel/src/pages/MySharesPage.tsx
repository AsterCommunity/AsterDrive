import { useCallback, useEffect, useState } from "react";
import { useTranslation } from "react-i18next";
import { toast } from "sonner";
import { ConfirmDialog } from "@/components/common/ConfirmDialog";
import { EmptyState } from "@/components/common/EmptyState";
import { EditShareDialog } from "@/components/files/EditShareDialog";
import { FileTypeIcon } from "@/components/files/FileTypeIcon";
import { AppLayout } from "@/components/layout/AppLayout";
import { Badge } from "@/components/ui/badge";
import { Button } from "@/components/ui/button";
import { Card } from "@/components/ui/card";
import {
	ContextMenu,
	ContextMenuContent,
	ContextMenuItem,
	ContextMenuSeparator,
	ContextMenuTrigger,
} from "@/components/ui/context-menu";
import { Icon } from "@/components/ui/icon";
import { ItemCheckbox } from "@/components/ui/item-checkbox";
import { handleApiError } from "@/hooks/useApiError";
import { useConfirmDialog } from "@/hooks/useConfirmDialog";
import { usePageTitle } from "@/hooks/usePageTitle";
import { useSelectionShortcuts } from "@/hooks/useSelectionShortcuts";
import { writeTextToClipboard } from "@/lib/clipboard";
import { PAGE_SECTION_PADDING_CLASS } from "@/lib/constants";
import { formatDateAbsolute } from "@/lib/format";
import { cn } from "@/lib/utils";
import { shareService } from "@/services/shareService";
import type { BatchResult, MyShareInfo, ShareStatus } from "@/types/api";

const PAGE_SIZE = 50;

export default function MySharesPage() {
	const { t } = useTranslation(["core", "share", "errors"]);
	usePageTitle(t("share:my_shares_title"));
	const [page, setPage] = useState(0);
	const [loading, setLoading] = useState(true);
	const [shares, setShares] = useState<MyShareInfo[]>([]);
	const [total, setTotal] = useState(0);
	const [selectedShareIds, setSelectedShareIds] = useState<Set<number>>(
		new Set(),
	);
	const [editTarget, setEditTarget] = useState<MyShareInfo | null>(null);

	const loadPage = useCallback(async (targetPage: number) => {
		try {
			setLoading(true);
			const data = await shareService.listMine({
				limit: PAGE_SIZE,
				offset: targetPage * PAGE_SIZE,
			});
			setShares(data.items);
			setTotal(data.total);
			setSelectedShareIds(new Set());
			return data;
		} catch (error) {
			handleApiError(error);
			return null;
		} finally {
			setLoading(false);
		}
	}, []);

	useEffect(() => {
		void loadPage(page);
	}, [loadPage, page]);

	const reloadCurrentPage = useCallback(async () => {
		const data = await loadPage(page);
		if (data && data.items.length === 0 && page > 0 && data.total > 0) {
			setPage((current) => Math.max(0, current - 1));
		}
	}, [loadPage, page]);
	const handleDelete = async (targets: MyShareInfo[]) => {
		if (targets.length === 0) return;
		try {
			if (targets.length === 1) {
				await shareService.delete(targets[0].id);
				toast.success(t("share:my_shares_delete_success"));
			} else {
				const result = await shareService.batchDelete(
					targets.map((target) => target.id),
				);
				showBatchDeleteToast(result);
			}

			clearSelection();
			await reloadCurrentPage();
		} catch (error) {
			handleApiError(error);
		}
	};
	const {
		confirmId: deleteTargets,
		requestConfirm: requestDeleteConfirm,
		dialogProps: deleteDialogProps,
	} = useConfirmDialog<MyShareInfo[]>(handleDelete);

	const totalPages = Math.max(1, Math.ceil(total / PAGE_SIZE));
	const selectedShares = shares.filter((share) =>
		selectedShareIds.has(share.id),
	);
	const selectedCount = selectedShares.length;
	const allSelected = shares.length > 0 && selectedCount === shares.length;
	const singleDeleteTarget =
		deleteTargets && deleteTargets.length === 1 ? deleteTargets[0] : null;

	const clearSelection = useCallback(() => {
		setSelectedShareIds(new Set());
	}, []);

	const selectAll = useCallback(() => {
		setSelectedShareIds(new Set(shares.map((share) => share.id)));
	}, [shares]);

	const toggleSelectAll = useCallback(() => {
		if (allSelected) {
			clearSelection();
			return;
		}
		selectAll();
	}, [allSelected, clearSelection, selectAll]);

	useSelectionShortcuts({
		selectAll,
		clearSelection,
		enabled: deleteTargets === null && editTarget === null,
	});

	const toggleSelectShare = (shareId: number) => {
		setSelectedShareIds((current) => {
			const next = new Set(current);
			if (next.has(shareId)) {
				next.delete(shareId);
			} else {
				next.add(shareId);
			}
			return next;
		});
	};

	const copyShareLink = async (share: MyShareInfo) => {
		try {
			await writeTextToClipboard(shareService.pageUrl(share.token));
			toast.success(t("copied_to_clipboard"));
		} catch {
			toast.error(t("errors:unexpected_error"));
		}
	};

	const openShareLink = (share: MyShareInfo) => {
		window.open(
			shareService.pagePath(share.token),
			"_blank",
			"noopener,noreferrer",
		);
	};

	const showBatchDeleteToast = (result: BatchResult) => {
		if (result.failed === 0) {
			toast.success(
				t("share:my_shares_batch_delete_success", {
					count: result.succeeded,
				}),
			);
			return;
		}

		if (result.succeeded === 0) {
			toast.error(t("share:my_shares_batch_delete_failed"));
			return;
		}

		toast.success(
			t("share:my_shares_batch_delete_partial", {
				succeeded: result.succeeded,
				failed: result.failed,
			}),
		);
	};

	const statusBadge = (status: ShareStatus) => {
		switch (status) {
			case "active":
				return <Badge variant="secondary">{t("active")}</Badge>;
			case "expired":
				return <Badge variant="outline">{t("expired")}</Badge>;
			case "exhausted":
				return (
					<Badge variant="outline">
						{t("share:my_shares_status_exhausted")}
					</Badge>
				);
			case "deleted":
				return (
					<Badge variant="destructive">
						{t("share:my_shares_status_deleted")}
					</Badge>
				);
		}
	};

	return (
		<AppLayout>
			<div className="flex min-h-0 flex-1 flex-col overflow-auto">
				<div
					className={`mx-auto flex w-full max-w-7xl flex-col gap-5 py-4 md:py-6 ${PAGE_SECTION_PADDING_CLASS}`}
				>
					<div className="flex flex-wrap items-center gap-3">
						<h1 className="text-2xl font-semibold tracking-tight">
							{t("share:my_shares_title")}
						</h1>
						<Button
							variant="ghost"
							size="icon-sm"
							onClick={() => void loadPage(page)}
							disabled={loading}
							aria-label={t("refresh")}
							title={t("refresh")}
						>
							<Icon
								name={loading ? "Spinner" : "ArrowsClockwise"}
								className={`h-4 w-4 ${loading ? "animate-spin" : ""}`}
							/>
						</Button>
						{shares.length > 0 && (
							<Button variant="outline" size="sm" onClick={toggleSelectAll}>
								{allSelected
									? t("share:my_shares_clear_selection")
									: t("share:my_shares_select_all")}
							</Button>
						)}
						{selectedCount > 0 && (
							<span className="text-sm text-muted-foreground">
								{t("core:selected_count", { count: selectedCount })}
							</span>
						)}
					</div>

					{loading ? (
						<div className="grid gap-3 sm:grid-cols-2 lg:grid-cols-3">
							{["s1", "s2", "s3", "s4", "s5", "s6"].map((key) => (
								<Card key={key} className="h-28 animate-pulse bg-muted/20" />
							))}
						</div>
					) : shares.length === 0 ? (
						<Card className="bg-muted/15">
							<div className="py-12">
								<EmptyState
									icon={<Icon name="Link" className="h-10 w-10" />}
									title={t("share:my_shares_empty_title")}
									description={t("share:my_shares_empty_desc")}
								/>
							</div>
						</Card>
					) : (
						<>
							<div className="grid gap-3 sm:grid-cols-2 lg:grid-cols-3">
								{shares.map((share) => {
									const isFolder = share.resource_type === "folder";
									const selected = selectedShareIds.has(share.id);

									return (
										<ContextMenu key={share.id}>
											<ContextMenuTrigger className="w-full">
												<Card
													className={cn(
														"cursor-pointer border bg-card/80 px-4 py-3 shadow-sm transition-all duration-150 hover:-translate-y-0.5 hover:bg-card hover:shadow-md dark:shadow-none dark:hover:shadow-none",
														selected && "border-primary bg-accent/35",
													)}
													onClick={() => openShareLink(share)}
													role="button"
													tabIndex={0}
													onKeyDown={(event) => {
														if (event.key === "Enter") {
															openShareLink(share);
														}
													}}
												>
													<div className="flex items-center gap-3">
														<ItemCheckbox
															checked={selected}
															onChange={() => toggleSelectShare(share.id)}
															className="mt-0.5"
														/>
														<div className="flex h-10 w-10 shrink-0 items-center justify-center rounded-xl bg-muted/45">
															{isFolder ? (
																<Icon
																	name="Folder"
																	className="h-5 w-5 text-amber-500"
																/>
															) : (
																<FileTypeIcon
																	mimeType=""
																	fileName={share.resource_name}
																	className="h-5 w-5"
																/>
															)}
														</div>
														<div className="min-w-0 flex-1">
															<span className="block truncate text-sm font-semibold">
																{share.resource_name}
															</span>
														</div>
														<div className="shrink-0">
															<div className="flex items-center gap-2">
																{statusBadge(share.status)}
															</div>
														</div>
													</div>

													<div className="mt-2 flex flex-wrap items-center gap-x-3 gap-y-1 pl-8 text-xs text-muted-foreground">
														<span>
															{t("share:my_shares_created_label", {
																date: formatDateAbsolute(share.created_at),
															})}
														</span>
														{share.expires_at ? (
															<span>
																{t("share:my_shares_expire_label", {
																	date: formatDateAbsolute(share.expires_at),
																})}
															</span>
														) : (
															<span>{t("share:my_shares_never")}</span>
														)}
														{share.has_password && (
															<Icon name="Lock" className="h-3 w-3" />
														)}
													</div>
												</Card>
											</ContextMenuTrigger>
											<ContextMenuContent>
												<ContextMenuItem onClick={() => setEditTarget(share)}>
													<Icon name="PencilSimple" />
													{t("core:edit")}
												</ContextMenuItem>
												<ContextMenuItem
													onClick={() => void copyShareLink(share)}
												>
													<Icon name="Copy" />
													{t("share:my_shares_card_copy")}
												</ContextMenuItem>
												<ContextMenuItem onClick={() => openShareLink(share)}>
													<Icon name="ArrowSquareOut" />
													{t("share:my_shares_card_open")}
												</ContextMenuItem>
												<ContextMenuSeparator />
												<ContextMenuItem
													variant="destructive"
													onClick={() => requestDeleteConfirm([share])}
												>
													<Icon name="Trash" />
													{t("share:my_shares_card_delete")}
												</ContextMenuItem>
											</ContextMenuContent>
										</ContextMenu>
									);
								})}
							</div>

							<div className="flex items-center justify-between rounded-xl border bg-muted/15 px-4 py-3">
								<p className="text-sm text-muted-foreground">
									{t("share:my_shares_pagination_desc", {
										current: page + 1,
										total: totalPages,
										count: total,
									})}
								</p>
								<div className="flex items-center gap-2">
									<Button
										variant="outline"
										size="sm"
										disabled={page === 0}
										onClick={() =>
											setPage((current) => Math.max(0, current - 1))
										}
									>
										{t("share:my_shares_prev")}
									</Button>
									<Button
										variant="outline"
										size="sm"
										disabled={page + 1 >= totalPages}
										onClick={() =>
											setPage((current) =>
												current + 1 >= totalPages ? current : current + 1,
											)
										}
									>
										{t("share:my_shares_next")}
									</Button>
								</div>
							</div>
						</>
					)}
				</div>
			</div>

			{selectedCount > 0 && (
				<div className="fixed bottom-4 left-1/2 z-50 flex -translate-x-1/2 items-center gap-2 rounded-xl border border-border/70 bg-card/95 px-4 py-2 shadow-lg shadow-black/8 backdrop-blur supports-[backdrop-filter]:bg-card/85 dark:shadow-none">
					<span className="text-sm font-medium">
						{t("core:selected_count", { count: selectedCount })}
					</span>
					<div className="flex items-center gap-1">
						{selectedCount === 1 && (
							<Button
								size="sm"
								variant="outline"
								onClick={() => setEditTarget(selectedShares[0])}
							>
								<Icon name="PencilSimple" className="mr-1 h-3.5 w-3.5" />
								{t("core:edit")}
							</Button>
						)}
						<Button
							size="sm"
							variant="destructive"
							onClick={() => requestDeleteConfirm(selectedShares)}
						>
							<Icon name="Trash" className="mr-1 h-3.5 w-3.5" />
							{t("share:my_shares_batch_delete")}
						</Button>
					</div>
					<Button size="sm" variant="ghost" onClick={clearSelection}>
						<Icon name="X" className="h-3.5 w-3.5" />
					</Button>
				</div>
			)}

			<ConfirmDialog
				{...deleteDialogProps}
				title={
					singleDeleteTarget
						? t("share:my_shares_delete_title", {
								name: singleDeleteTarget.resource_name,
							})
						: t("share:my_shares_batch_delete_title", {
								count: deleteTargets?.length ?? 0,
							})
				}
				description={
					singleDeleteTarget
						? t("share:my_shares_delete_desc")
						: t("share:my_shares_batch_delete_desc")
				}
				confirmLabel={t("delete")}
				variant="destructive"
			/>

			<EditShareDialog
				open={editTarget !== null}
				onOpenChange={(open) => {
					if (!open) setEditTarget(null);
				}}
				share={editTarget}
				onSaved={() => reloadCurrentPage()}
			/>
		</AppLayout>
	);
}
