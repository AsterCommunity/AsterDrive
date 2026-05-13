import type { TFunction } from "i18next";
import { type DragEvent, useCallback, useState } from "react";
import { toast } from "sonner";
import { handleApiError } from "@/hooks/useApiError";
import {
	DRAG_SOURCE_MIME,
	FILE_BROWSER_FEEDBACK_DURATION_MS,
} from "@/lib/constants";
import {
	getInvalidInternalDropReason,
	hasInternalDragData,
	type InternalDragData,
	readInternalDragData,
} from "@/lib/dragDrop";
import { formatBatchToast } from "@/lib/formatBatchToast";
import {
	forgetStorageEventEchoes,
	rememberStorageDeleteEchoes,
} from "@/lib/storageEventEcho";
import { batchService } from "@/services/batchService";
import type { BreadcrumbItem } from "@/stores/fileStore";
import { useWorkspaceStore } from "@/stores/workspaceStore";
import type { BatchResult } from "@/types/api";

interface UseFileBrowserDragAndDropOptions {
	breadcrumb: BreadcrumbItem[];
	clearSelection: () => void;
	folderId: number | null;
	isSearching: boolean;
	moveToFolder: (
		fileIds: number[],
		folderIds: number[],
		targetFolderId: number | null,
	) => Promise<BatchResult>;
	refresh: () => Promise<void>;
	search: (query: string) => Promise<void>;
	searchQuery: string | null;
	t: TFunction;
}

function createIdSet(ids: number[]) {
	return new Set(ids);
}

export function useFileBrowserDragAndDrop({
	breadcrumb,
	clearSelection,
	folderId,
	isSearching,
	moveToFolder,
	refresh,
	search,
	searchQuery,
	t,
}: UseFileBrowserDragAndDropOptions) {
	const [contentDragOver, setContentDragOver] = useState(false);
	const [dragOverBreadcrumbIndex, setDragOverBreadcrumbIndex] = useState<
		number | null
	>(null);
	const [fadingFileIds, setFadingFileIds] = useState<Set<number>>(new Set());
	const [fadingFolderIds, setFadingFolderIds] = useState<Set<number>>(
		new Set(),
	);

	const clearFadingState = useCallback(() => {
		setFadingFileIds(new Set());
		setFadingFolderIds(new Set());
	}, []);

	const showBatchToast = useCallback(
		(action: "move" | "delete", result: BatchResult) => {
			const batchToast = formatBatchToast(t, action, result);
			if (batchToast.variant === "error") {
				toast.error(batchToast.title, {
					description: batchToast.description,
				});
				return;
			}

			toast.success(batchToast.title, {
				description: batchToast.description,
			});
		},
		[t],
	);

	const handleMoveToFolder = useCallback(
		async (
			fileIds: number[],
			folderIds: number[],
			targetFolderId: number | null,
		) => {
			try {
				setFadingFileIds(createIdSet(fileIds));
				setFadingFolderIds(createIdSet(folderIds));
				const result = await moveToFolder(fileIds, folderIds, targetFolderId);
				document.dispatchEvent(
					new CustomEvent("folder-tree-move", {
						detail: { folderIds, targetFolderId },
					}),
				);
				await new Promise((resolve) =>
					setTimeout(resolve, FILE_BROWSER_FEEDBACK_DURATION_MS),
				);
				clearFadingState();
				showBatchToast("move", result);
			} catch (err) {
				clearFadingState();
				handleApiError(err);
			}
		},
		[clearFadingState, moveToFolder, showBatchToast],
	);

	const handleBreadcrumbDragOver = useCallback(
		(event: DragEvent, index: number) => {
			if (!hasInternalDragData(event.dataTransfer)) return;
			event.preventDefault();
			event.dataTransfer.dropEffect = "move";
			setDragOverBreadcrumbIndex(index);
		},
		[],
	);

	const handleBreadcrumbDragLeave = useCallback((event: DragEvent) => {
		const nextTarget = event.relatedTarget;
		if (
			nextTarget instanceof Node &&
			event.currentTarget.contains(nextTarget)
		) {
			return;
		}
		setDragOverBreadcrumbIndex(null);
	}, []);

	const handleBreadcrumbDrop = useCallback(
		async (event: DragEvent, index: number, targetFolderId: number | null) => {
			setDragOverBreadcrumbIndex(null);
			event.preventDefault();
			const data = readInternalDragData(event.dataTransfer);
			if (!data) return;

			const targetPathIds = breadcrumb
				.slice(0, index + 1)
				.map((item) => item.id)
				.filter((id): id is number => id !== null);
			if (
				getInvalidInternalDropReason(data, targetFolderId, targetPathIds) !==
				null
			) {
				return;
			}

			await handleMoveToFolder(data.fileIds, data.folderIds, targetFolderId);
		},
		[breadcrumb, handleMoveToFolder],
	);

	const handleContentDragOver = useCallback(
		(event: DragEvent<HTMLElement>) => {
			const isTreeDrag = event.dataTransfer.types.includes(DRAG_SOURCE_MIME);
			if (
				!hasInternalDragData(event.dataTransfer) ||
				isSearching ||
				!isTreeDrag
			) {
				setContentDragOver(false);
				return;
			}
			event.preventDefault();
			event.dataTransfer.dropEffect = "move";
			setContentDragOver(true);
		},
		[isSearching],
	);

	const handleContentDragLeave = useCallback(
		(event: DragEvent<HTMLElement>) => {
			const nextTarget = event.relatedTarget;
			if (
				nextTarget instanceof Node &&
				event.currentTarget.contains(nextTarget)
			) {
				return;
			}
			setContentDragOver(false);
		},
		[],
	);

	const handleContentDrop = useCallback(
		async (event: DragEvent<HTMLElement>) => {
			setContentDragOver(false);
			if (isSearching || !event.dataTransfer.types.includes(DRAG_SOURCE_MIME)) {
				return;
			}
			event.preventDefault();
			const data = readInternalDragData(event.dataTransfer);
			if (!data) return;
			const currentPathIds = breadcrumb
				.map((item) => item.id)
				.filter((id): id is number => id !== null);
			if (
				getInvalidInternalDropReason(data, folderId, currentPathIds) !== null
			) {
				return;
			}
			await handleMoveToFolder(data.fileIds, data.folderIds, folderId);
		},
		[breadcrumb, folderId, handleMoveToFolder, isSearching],
	);

	const handleTrashDrop = useCallback(
		async ({ fileIds, folderIds }: InternalDragData) => {
			if (fileIds.length === 0 && folderIds.length === 0) return;
			const echoIds = rememberStorageDeleteEchoes({
				workspace: useWorkspaceStore.getState().workspace,
				fileIds,
				folderIds,
			});
			try {
				setFadingFileIds(createIdSet(fileIds));
				setFadingFolderIds(createIdSet(folderIds));
				const result = await batchService.batchDelete(fileIds, folderIds);
				await new Promise((resolve) =>
					setTimeout(resolve, FILE_BROWSER_FEEDBACK_DURATION_MS),
				);
				clearFadingState();
				showBatchToast("delete", result);
				clearSelection();
				await (searchQuery ? search(searchQuery) : refresh());
			} catch (err) {
				forgetStorageEventEchoes(echoIds);
				clearFadingState();
				handleApiError(err);
			}
		},
		[
			clearFadingState,
			clearSelection,
			refresh,
			search,
			searchQuery,
			showBatchToast,
		],
	);

	return {
		contentDragOver,
		dragOverBreadcrumbIndex,
		fadingFileIds,
		fadingFolderIds,
		handleBreadcrumbDragLeave,
		handleBreadcrumbDragOver,
		handleBreadcrumbDrop,
		handleContentDragLeave,
		handleContentDragOver,
		handleContentDrop,
		handleMoveToFolder,
		handleTrashDrop,
	};
}
