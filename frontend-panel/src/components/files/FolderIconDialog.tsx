import { useRef, useState } from "react";
import { useTranslation } from "react-i18next";
import { toast } from "sonner";
import { FolderIconRenderer } from "@/components/files/FolderIconRenderer";
import {
	folderBuiltinIconKeys,
	folderIconCatalog,
} from "@/components/files/folderIconCatalog";
import { Button } from "@/components/ui/button";
import {
	Dialog,
	DialogContent,
	DialogFooter,
	DialogHeader,
	DialogTitle,
} from "@/components/ui/dialog";
import { Input } from "@/components/ui/input";
import { handleApiError } from "@/hooks/useApiError";
import { cn } from "@/lib/utils";
import { fileService } from "@/services/fileService";
import type {
	FolderBuiltinIcon,
	FolderIcon,
	FolderListItem,
} from "@/types/api";

interface FolderIconDialogProps {
	open: boolean;
	onOpenChange: (open: boolean) => void;
	onOpenChangeComplete?: (open: boolean) => void;
	folder: FolderListItem | null;
	onUpdated?: () => void | Promise<void>;
}

type IconMode = FolderIcon["kind"];

export function FolderIconDialog({
	open,
	onOpenChange,
	onOpenChangeComplete,
	folder,
	onUpdated,
}: FolderIconDialogProps) {
	const { t } = useTranslation("files");
	const initial = folder?.icon ?? ({ kind: "default" } as const);
	const resetKey = `${folder?.id ?? "none"}:${JSON.stringify(initial)}:${open}`;

	return (
		<Dialog
			open={open}
			onOpenChange={onOpenChange}
			onOpenChangeComplete={onOpenChangeComplete}
		>
			<DialogContent keepMounted className="sm:max-w-md">
				<DialogHeader>
					<DialogTitle>{t("folder_icon_title")}</DialogTitle>
				</DialogHeader>
				{folder ? (
					<FolderIconDialogForm
						key={resetKey}
						folder={folder}
						onOpenChange={onOpenChange}
						onUpdated={onUpdated}
					/>
				) : null}
			</DialogContent>
		</Dialog>
	);
}

function FolderIconDialogForm({
	folder,
	onOpenChange,
	onUpdated,
}: Pick<FolderIconDialogProps, "onOpenChange" | "onUpdated"> & {
	folder: FolderListItem;
}) {
	const { t } = useTranslation("files");
	const [mode, setMode] = useState<IconMode>(folder.icon.kind);
	const [builtin, setBuiltin] = useState<FolderBuiltinIcon>(
		folder.icon.kind === "builtin" ? folder.icon.key : "documents",
	);
	const [emoji, setEmoji] = useState(
		folder.icon.kind === "emoji" ? folder.icon.value : "",
	);
	const [submitting, setSubmitting] = useState(false);
	const submittingRef = useRef(false);

	const nextIcon: FolderIcon =
		mode === "default"
			? { kind: "default" }
			: mode === "builtin"
				? { kind: "builtin", key: builtin }
				: { kind: "emoji", value: emoji.trim() };
	const disabled =
		submitting || (mode === "emoji" && emoji.trim().length === 0);

	const save = async () => {
		if (disabled || submittingRef.current) return;
		submittingRef.current = true;
		setSubmitting(true);
		try {
			await fileService.setFolderIcon(folder.id, nextIcon);
			toast.success(t("folder_icon_updated"));
			onOpenChange(false);
			await onUpdated?.();
		} catch (error) {
			handleApiError(error);
			submittingRef.current = false;
			setSubmitting(false);
		}
	};

	return (
		<>
			<div className="grid grid-cols-3 gap-1 rounded-lg bg-muted p-1">
				{(["default", "builtin", "emoji"] as const).map((value) => (
					<Button
						key={value}
						type="button"
						variant={mode === value ? "secondary" : "ghost"}
						className="h-8"
						onClick={() => setMode(value)}
					>
						{t(`folder_icon_mode_${value}`)}
					</Button>
				))}
			</div>

			{mode === "default" ? (
				<div className="flex h-28 items-center justify-center rounded-lg border border-border/70 bg-muted/20">
					<FolderIconRenderer className="size-16" />
				</div>
			) : null}

			{mode === "builtin" ? (
				<div className="grid grid-cols-4 gap-2 sm:grid-cols-6">
					{folderBuiltinIconKeys.map((key) => {
						const CatalogIcon = folderIconCatalog[key];
						return (
							<button
								key={key}
								type="button"
								className={cn(
									"flex aspect-square items-center justify-center rounded-md border bg-background transition-colors hover:bg-accent focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring",
									builtin === key && "border-primary ring-2 ring-primary/25",
								)}
								aria-label={t(`folder_icon_builtin_${key}`)}
								aria-pressed={builtin === key}
								onClick={() => setBuiltin(key)}
							>
								<CatalogIcon className="size-8" />
							</button>
						);
					})}
				</div>
			) : null}

			{mode === "emoji" ? (
				<div className="flex items-center gap-3">
					<div className="flex size-16 shrink-0 items-center justify-center rounded-md border bg-muted/20 text-4xl">
						{emoji || "🙂"}
					</div>
					<Input
						value={emoji}
						maxLength={64}
						autoFocus
						aria-label={t("folder_icon_emoji_input")}
						placeholder={t("folder_icon_emoji_placeholder")}
						onChange={(event) => setEmoji(event.target.value)}
					/>
				</div>
			) : null}

			<DialogFooter>
				<Button type="button" onClick={() => void save()} disabled={disabled}>
					{t("folder_icon_save")}
				</Button>
			</DialogFooter>
		</>
	);
}
