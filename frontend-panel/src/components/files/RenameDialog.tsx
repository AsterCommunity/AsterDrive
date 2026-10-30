import type { FormEvent } from "react";
import { useRef, useState } from "react";
import { useTranslation } from "react-i18next";
import { toast } from "sonner";
import { Button } from "@/components/ui/button";
import {
	Dialog,
	DialogContent,
	DialogHeader,
	DialogTitle,
} from "@/components/ui/dialog";
import {
	DialogLifecycle,
	useDialogActivity,
} from "@/components/ui/dialog-lifecycle";
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
	onRenamed?: () => Promise<void> | void;
}

export function RenameDialog(props: RenameDialogProps) {
	return (
		<DialogLifecycle
			open={props.open}
			identity={`${props.type}:${props.id}`}
			target={props}
			onOpenChange={props.onOpenChange}
		>
			{({ target, active, onOpenChange }) => (
				<RenameDialogContent
					{...target}
					open={active}
					onOpenChange={onOpenChange}
				/>
			)}
		</DialogLifecycle>
	);
}

function RenameDialogContent({
	open,
	onOpenChange,
	type,
	id,
	currentName,
	onRenamed,
}: RenameDialogProps) {
	const resetKey = `${type}:${id}:${currentName}`;

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
					onRenamed={onRenamed}
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
	onRenamed,
	onOpenChange,
}: Omit<RenameDialogProps, "open">) {
	const isCurrent = useDialogActivity();
	const { t } = useTranslation("files");
	const refresh = useFileStore((s) => s.refresh);
	const [name, setName] = useState(currentName);
	const [isSubmitting, setIsSubmitting] = useState(false);
	const submittingRef = useRef(false);

	const handleSubmit = async (e: FormEvent) => {
		e.preventDefault();
		if (submittingRef.current) {
			return;
		}
		const trimmed = name.trim();
		if (!trimmed || trimmed === currentName) {
			onOpenChange(false);
			return;
		}
		submittingRef.current = true;
		setIsSubmitting(true);
		try {
			if (type === "file") {
				await fileService.renameFile(id, trimmed);
			} else {
				await fileService.renameFolder(id, trimmed);
			}
		} catch (error) {
			if (isCurrent()) handleApiError(error);
			submittingRef.current = false;
			if (isCurrent()) setIsSubmitting(false);
			return;
		}
		try {
			toast.success(t("rename_success"));
			onOpenChange(false);
			if (onRenamed) {
				await onRenamed();
			} else {
				await refresh();
			}
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
			<Button type="submit" className="w-full" disabled={isSubmitting}>
				{t("rename")}
			</Button>
		</form>
	);
}
