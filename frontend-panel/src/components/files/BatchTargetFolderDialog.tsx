import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import { useTranslation } from "react-i18next";
import { toast } from "sonner";
import { SkeletonTree } from "@/components/common/SkeletonTree";
import {
	Breadcrumb,
	BreadcrumbItem,
	BreadcrumbLink,
	BreadcrumbList,
	BreadcrumbSeparator,
} from "@/components/ui/breadcrumb";
import { Button } from "@/components/ui/button";
import {
	Dialog,
	DialogContent,
	DialogDescription,
	DialogFooter,
	DialogHeader,
	DialogTitle,
} from "@/components/ui/dialog";
import { Icon } from "@/components/ui/icon";
import { Input } from "@/components/ui/input";
import { ScrollArea } from "@/components/ui/scroll-area";
import { handleApiError } from "@/hooks/useApiError";
import { FOLDER_LIMIT } from "@/lib/constants";
import { isImeComposingKeyEvent } from "@/lib/keyboard";
import { cn } from "@/lib/utils";
import { fileService } from "@/services/fileService";
import type { BreadcrumbItem as FileBreadcrumbItem } from "@/stores/fileStore";
import type { FolderListItem } from "@/types/api";

const EMPTY_SELECTED_FOLDER_IDS: number[] = [];

interface BatchTargetFolderDialogProps {
	open: boolean;
	onOpenChange: (open: boolean) => void;
	mode: "move" | "copy";
	onConfirm: (targetFolderId: number | null) => Promise<void>;
	currentFolderId: number | null;
	initialBreadcrumb: FileBreadcrumbItem[];
	selectedFolderIds?: number[];
}

export function BatchTargetFolderDialog({
	open,
	onOpenChange,
	mode,
	onConfirm,
	currentFolderId,
	initialBreadcrumb,
	selectedFolderIds = EMPTY_SELECTED_FOLDER_IDS,
}: BatchTargetFolderDialogProps) {
	const { t } = useTranslation(["files", "core"]);
	const [loading, setLoading] = useState(false);
	const [submitting, setSubmitting] = useState(false);
	const [creatingFolder, setCreatingFolder] = useState(false);
	const [showCreateFolder, setShowCreateFolder] = useState(false);
	const [newFolderName, setNewFolderName] = useState("");
	const [folders, setFolders] = useState<FolderListItem[]>([]);
	const [activeFolderId, setActiveFolderId] = useState<number | null>(null);
	const createFolderInputComposingRef = useRef(false);
	const createFolderInputCompositionEndAtRef = useRef(0);
	const [breadcrumb, setBreadcrumb] = useState<FileBreadcrumbItem[]>([
		{ id: null, name: t("files:root") },
	]);

	const title = useMemo(
		() => (mode === "move" ? t("files:batch_move") : t("files:batch_copy")),
		[mode, t],
	);

	const confirmLabel = useMemo(
		() =>
			mode === "move"
				? t("files:move_to_current_folder")
				: t("files:copy_to_current_folder"),
		[mode, t],
	);

	const loadFolder = useCallback(async (folderId: number | null) => {
		setLoading(true);
		try {
			const folderOnlyParams = { file_limit: 0, folder_limit: FOLDER_LIMIT };
			const contents =
				folderId === null
					? await fileService.listRoot(folderOnlyParams)
					: await fileService.listFolder(folderId, folderOnlyParams);
			setFolders(contents.folders);
			setActiveFolderId(folderId);
		} catch (error) {
			handleApiError(error);
		} finally {
			setLoading(false);
		}
	}, []);

	useEffect(() => {
		if (!open) return;

		createFolderInputComposingRef.current = false;
		createFolderInputCompositionEndAtRef.current = 0;

		const normalizedBreadcrumb =
			initialBreadcrumb.length > 0
				? initialBreadcrumb
				: [{ id: null, name: t("files:root") }];

		setBreadcrumb(normalizedBreadcrumb);
		setShowCreateFolder(false);
		setNewFolderName("");
		loadFolder(currentFolderId);
	}, [open, currentFolderId, initialBreadcrumb, loadFolder, t]);

	const navigateTo = async (folder: FolderListItem) => {
		const existingIndex = breadcrumb.findIndex((item) => item.id === folder.id);
		if (existingIndex >= 0) {
			setBreadcrumb((prev) => prev.slice(0, existingIndex + 1));
		} else {
			setBreadcrumb((prev) => [...prev, { id: folder.id, name: folder.name }]);
		}
		await loadFolder(folder.id);
	};

	const navigateBreadcrumb = async (
		item: FileBreadcrumbItem,
		index: number,
	) => {
		setBreadcrumb((prev) => prev.slice(0, index + 1));
		await loadFolder(item.id);
	};

	const handleGoUp = async () => {
		if (breadcrumb.length <= 1) return;
		const parent = breadcrumb[breadcrumb.length - 2];
		await navigateBreadcrumb(parent, breadcrumb.length - 2);
	};

	const targetPathIds = breadcrumb
		.map((item) => item.id)
		.filter((id): id is number => id !== null);

	const validationMessage =
		selectedFolderIds.length > 0 &&
		selectedFolderIds.some((folderId) => targetPathIds.includes(folderId))
			? t("files:batch_target_invalid_descendant")
			: null;

	const handleCreateFolder = async () => {
		const trimmedName = newFolderName.trim();
		if (!trimmedName) return;
		setCreatingFolder(true);
		try {
			await fileService.createFolder(trimmedName, activeFolderId);
			toast.success(t("files:create_folder_success"));
			setNewFolderName("");
			setShowCreateFolder(false);
			await loadFolder(activeFolderId);
		} catch (error) {
			handleApiError(error);
		} finally {
			setCreatingFolder(false);
		}
	};

	const handleConfirm = async () => {
		if (validationMessage) return;
		setSubmitting(true);
		try {
			await onConfirm(activeFolderId);
			onOpenChange(false);
		} finally {
			setSubmitting(false);
		}
	};

	return (
		<Dialog open={open} onOpenChange={onOpenChange}>
			<DialogContent
				keepMounted
				className="max-w-2xl p-0 gap-0 overflow-hidden"
				showCloseButton={false}
			>
				<DialogHeader className="p-4 pb-3">
					<div className="flex items-start justify-between gap-3 pr-10">
						<div className="space-y-1">
							<DialogTitle>{title}</DialogTitle>
							<DialogDescription>
								{t("files:batch_target_folder_desc")}
							</DialogDescription>
						</div>
						<Button
							variant="outline"
							size="sm"
							onClick={() => {
								createFolderInputComposingRef.current = false;
								createFolderInputCompositionEndAtRef.current = 0;
								setShowCreateFolder((prev) => !prev);
							}}
							disabled={loading || submitting || creatingFolder}
						>
							<Icon name="FolderPlus" className="mr-1 size-3.5" />
							{t("files:create_folder")}
						</Button>
					</div>
					<Button
						variant="ghost"
						size="icon-sm"
						className="absolute top-3 right-3"
						onClick={() => onOpenChange(false)}
					>
						<Icon name="X" className="size-4" />
					</Button>
				</DialogHeader>

				<div className="px-4 pb-3 space-y-3">
					{showCreateFolder && (
						<div className="rounded-lg border bg-muted/20 p-3 space-y-3">
							<Input
								placeholder={t("files:folder_name")}
								value={newFolderName}
								onChange={(e) => setNewFolderName(e.target.value)}
								onCompositionStart={() => {
									createFolderInputComposingRef.current = true;
								}}
								onCompositionEnd={(e) => {
									createFolderInputComposingRef.current = false;
									createFolderInputCompositionEndAtRef.current = Date.now();
									setNewFolderName(e.currentTarget.value);
								}}
								onBlur={() => {
									createFolderInputComposingRef.current = false;
								}}
								onKeyDown={(e) => {
									if (
										createFolderInputComposingRef.current ||
										isImeComposingKeyEvent(e, {
											lastCompositionEndAt:
												createFolderInputCompositionEndAtRef.current,
										})
									) {
										return;
									}

									if (e.key === "Enter") {
										e.preventDefault();
										handleCreateFolder();
									}
								}}
								autoFocus
							/>
							<div className="flex items-center justify-end gap-2">
								<Button
									variant="outline"
									size="sm"
									onClick={() => {
										createFolderInputComposingRef.current = false;
										createFolderInputCompositionEndAtRef.current = 0;
										setShowCreateFolder(false);
										setNewFolderName("");
									}}
								>
									{t("core:cancel")}
								</Button>
								<Button
									size="sm"
									onClick={handleCreateFolder}
									disabled={creatingFolder || !newFolderName.trim()}
								>
									{creatingFolder
										? t("files:processing")
										: t("files:create_folder")}
								</Button>
							</div>
						</div>
					)}
					<Breadcrumb>
						<BreadcrumbList>
							{breadcrumb.map((item, index) => (
								<BreadcrumbItem key={item.id ?? "root"}>
									{index > 0 && <BreadcrumbSeparator />}
									{index < breadcrumb.length - 1 ? (
										<BreadcrumbLink
											className="cursor-pointer"
											onClick={() => navigateBreadcrumb(item, index)}
										>
											{item.name}
										</BreadcrumbLink>
									) : (
										<span className="font-medium text-foreground">
											{item.name}
										</span>
									)}
								</BreadcrumbItem>
							))}
						</BreadcrumbList>
					</Breadcrumb>
				</div>

				<ScrollArea className="h-[320px] border-y">
					<div className="p-3 h-full">
						{loading ? (
							<SkeletonTree count={6} />
						) : folders.length === 0 ? (
							<div className="flex h-full flex-col items-center justify-center px-6 text-center text-sm text-muted-foreground">
								<div className="font-medium text-foreground">
									{t("files:batch_target_empty")}
								</div>
								<div className="mt-2 max-w-md">
									{t("files:batch_target_empty_desc")}
								</div>
								{breadcrumb.length > 1 && (
									<Button
										variant="outline"
										size="sm"
										className="mt-4"
										onClick={handleGoUp}
									>
										<Icon name="ArrowUp" className="mr-1 size-3.5" />
										{t("files:batch_target_back")}
									</Button>
								)}
							</div>
						) : (
							<div className="space-y-1">
								{folders.map((folder) => (
									<button
										key={folder.id}
										type="button"
										className={cn(
											"flex w-full items-center gap-2 rounded-md px-3 py-2 text-left text-sm hover:bg-accent transition-colors",
											folder.id === activeFolderId && "bg-accent",
										)}
										onClick={() => navigateTo(folder)}
									>
										<Icon
											name="Folder"
											className="size-4 shrink-0 text-muted-foreground"
										/>
										<span className="truncate">{folder.name}</span>
										<Icon
											name="CaretRight"
											className="ml-auto size-3.5 text-muted-foreground"
										/>
									</button>
								))}
							</div>
						)}
					</div>
				</ScrollArea>

				<DialogFooter className="mt-0 -mx-0 -mb-0 rounded-none px-4 py-3">
					<div className="mr-auto flex min-h-9 items-center text-xs text-muted-foreground">
						<div>
							{t("files:batch_target_current_folder", {
								name:
									breadcrumb[breadcrumb.length - 1]?.name ?? t("files:root"),
							})}
						</div>
						{validationMessage && (
							<div className="text-destructive">{validationMessage}</div>
						)}
					</div>
					<Button variant="outline" onClick={() => onOpenChange(false)}>
						{t("core:cancel")}
					</Button>
					<Button
						onClick={handleConfirm}
						disabled={submitting || loading || !!validationMessage}
					>
						{submitting ? t("files:processing") : confirmLabel}
					</Button>
				</DialogFooter>
			</DialogContent>
		</Dialog>
	);
}
