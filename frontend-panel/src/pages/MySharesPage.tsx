import { useCallback, useEffect } from "react";
import { useTranslation } from "react-i18next";
import { toast } from "sonner";
import { ConfirmDialog } from "@/components/common/ConfirmDialog";
import { EditShareDialog } from "@/components/files/EditShareDialog";
import { AppLayout } from "@/components/layout/AppLayout";
import { Button } from "@/components/ui/button";
import { Icon } from "@/components/ui/icon";
import { handleApiError } from "@/hooks/useApiError";
import { useBottomOverlayOffset } from "@/hooks/useBottomOverlayOffset";
import { useConfirmDialog } from "@/hooks/useConfirmDialog";
import { usePageTitle } from "@/hooks/usePageTitle";
import { useSelectionShortcuts } from "@/hooks/useSelectionShortcuts";
import { writeTextToClipboard } from "@/lib/clipboard";
import {
	getBottomOverlayPaddingClass,
	PAGE_SECTION_PADDING_CLASS,
} from "@/lib/constants";
import { cn } from "@/lib/utils";
import { shareService } from "@/services/shareService";
import type { BatchResult, MyShareInfo } from "@/types/api";
import { MySharesContent } from "./my-shares/MySharesContent";
import { MySharesSelectionBar } from "./my-shares/MySharesSelectionBar";
import { useMySharesPageState } from "./my-shares/useMySharesPageState";

const PAGE_SIZE = 50;

function openShareLink(share: MyShareInfo) {
	window.open(
		shareService.pagePath(share.token),
		"_blank",
		"noopener,noreferrer",
	);
}

export default function MySharesPage() {
	const { t } = useTranslation(["core", "share", "errors"]);
	usePageTitle(t("share:my_shares_title"));
	const {
		clearSelection,
		editTarget,
		finishLoading,
		loading,
		page,
		selectAll: selectShareIds,
		selectedShareIds,
		setEditTarget,
		setPage,
		setPageData,
		shares,
		startLoading,
		toggleShareSelection,
		total,
	} = useMySharesPageState();

	const loadPage = useCallback(
		async (targetPage: number) => {
			try {
				startLoading();
				const data = await shareService.listMine({
					limit: PAGE_SIZE,
					offset: targetPage * PAGE_SIZE,
				});
				setPageData(data.items, data.total);
				return data;
			} catch (error) {
				handleApiError(error);
				return null;
			} finally {
				finishLoading();
			}
		},
		[finishLoading, setPageData, startLoading],
	);

	useEffect(() => {
		void loadPage(page);
	}, [loadPage, page]);

	const reloadCurrentPage = useCallback(async () => {
		const data = await loadPage(page);
		if (data && data.items.length === 0 && page > 0 && data.total > 0) {
			setPage((current) => Math.max(0, current - 1));
		}
	}, [loadPage, page, setPage]);
	const handleDelete = async (targets: MyShareInfo[]) => {
		if (targets.length === 0) return;
		try {
			if (targets.length === 1) {
				await shareService.delete(targets[0].id);
				toast.success(t("share:my_shares_delete_success"));
			} else {
				const result = await shareService.batchDelete({
					share_ids: targets.map((target) => target.id),
				});
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
	const bottomOverlayOffset = useBottomOverlayOffset(selectedCount > 0);

	const selectAll = useCallback(() => {
		selectShareIds(shares.map((share) => share.id));
	}, [selectShareIds, shares]);

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
		toggleShareSelection(shareId);
	};

	const copyShareLink = async (share: MyShareInfo) => {
		try {
			await writeTextToClipboard(shareService.pageUrl(share.token));
			toast.success(t("copied_to_clipboard"));
		} catch {
			toast.error(t("errors:unexpected_error"));
		}
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

	return (
		<AppLayout>
			<div
				data-testid="my-shares-scroll-container"
				className={cn(
					"flex min-h-0 flex-1 flex-col overflow-auto",
					getBottomOverlayPaddingClass(bottomOverlayOffset),
				)}
			>
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
								className={`size-4 ${loading ? "animate-spin" : ""}`}
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

					<MySharesContent
						loading={loading}
						shares={shares}
						selectedShareIds={selectedShareIds}
						page={page}
						totalPages={totalPages}
						onToggleSelect={toggleSelectShare}
						onOpen={openShareLink}
						onEdit={setEditTarget}
						onCopy={(share) => void copyShareLink(share)}
						onDelete={(share) => requestDeleteConfirm([share])}
						onPrevPage={() => setPage((current) => Math.max(0, current - 1))}
						onNextPage={() =>
							setPage((current) =>
								current + 1 >= totalPages ? current : current + 1,
							)
						}
						labels={{
							active: t("active"),
							copy: t("share:my_shares_card_copy"),
							created: (date) => t("share:my_shares_created_label", { date }),
							delete: t("share:my_shares_card_delete"),
							deleted: t("share:my_shares_status_deleted"),
							edit: t("core:edit"),
							emptyDescription: t("share:my_shares_empty_desc"),
							emptyTitle: t("share:my_shares_empty_title"),
							exhausted: t("share:my_shares_status_exhausted"),
							expire: (date) => t("share:my_shares_expire_label", { date }),
							expired: t("expired"),
							never: t("share:my_shares_never"),
							next: t("share:my_shares_next"),
							open: t("share:my_shares_card_open"),
							pageDescription: t("share:my_shares_pagination_desc", {
								current: page + 1,
								total: totalPages,
								count: total,
							}),
							prev: t("share:my_shares_prev"),
						}}
					/>
				</div>
			</div>

			<MySharesSelectionBar
				selectedShares={selectedShares}
				selectedCountLabel={t("core:selected_count", { count: selectedCount })}
				editLabel={t("core:edit")}
				batchDeleteLabel={t("share:my_shares_batch_delete")}
				onEdit={setEditTarget}
				onDelete={requestDeleteConfirm}
				onClear={clearSelection}
			/>

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
