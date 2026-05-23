import type { FormEvent } from "react";
import { useState } from "react";
import { useTranslation } from "react-i18next";
import { toast } from "sonner";
import { Button } from "@/components/ui/button";
import {
	Dialog,
	DialogContent,
	DialogHeader,
	DialogTitle,
} from "@/components/ui/dialog";
import { Input } from "@/components/ui/input";
import { handleApiError } from "@/hooks/useApiError";
import { fileService } from "@/services/fileService";
import { useFileStore } from "@/stores/fileStore";

interface RenameDialogProps {
	open: boolean;
	onOpenChange: (open: boolean) => void;
	type: "file" | "folder";
	id: number;
	currentName: string;
}

export function RenameDialog({
	open,
	onOpenChange,
	type,
	id,
	currentName,
}: RenameDialogProps) {
	const resetKey = `${type}:${id}:${currentName}:${open ? "open" : "closed"}`;

	return (
		<Dialog open={open} onOpenChange={onOpenChange}>
			<DialogContent keepMounted>
				<DialogHeader>
					<DialogTitle>
						<RenameDialogTitle />
					</DialogTitle>
				</DialogHeader>
				<RenameDialogForm
					key={resetKey}
					type={type}
					id={id}
					currentName={currentName}
					onOpenChange={onOpenChange}
				/>
			</DialogContent>
		</Dialog>
	);
}

function RenameDialogTitle() {
	const { t } = useTranslation("files");

	return t("rename");
}

function RenameDialogForm({
	type,
	id,
	currentName,
	onOpenChange,
}: Omit<RenameDialogProps, "open">) {
	const { t } = useTranslation("files");
	const refresh = useFileStore((s) => s.refresh);
	const [name, setName] = useState(currentName);

	const handleSubmit = async (e: FormEvent) => {
		e.preventDefault();
		const trimmed = name.trim();
		if (!trimmed || trimmed === currentName) {
			onOpenChange(false);
			return;
		}
		try {
			if (type === "file") {
				await fileService.renameFile(id, trimmed);
			} else {
				await fileService.renameFolder(id, trimmed);
			}
			toast.success(t("rename_success"));
			onOpenChange(false);
			await refresh();
		} catch (error) {
			handleApiError(error);
		}
	};

	return (
		<form onSubmit={handleSubmit} className="space-y-4">
			<Input
				value={name}
				aria-label={t("rename")}
				onChange={(e) => setName(e.target.value)}
				autoFocus
				onFocus={(e) => {
					// Select filename without extension
					const dot = e.target.value.lastIndexOf(".");
					if (dot > 0 && type === "file") {
						e.target.setSelectionRange(0, dot);
					} else {
						e.target.select();
					}
				}}
			/>
			<Button type="submit" className="w-full">
				{t("rename")}
			</Button>
		</form>
	);
}
