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
import { useFileStore } from "@/stores/fileStore";

interface CreateFileDialogProps {
	open: boolean;
	onOpenChange: (open: boolean) => void;
}

export function CreateFileDialog(props: CreateFileDialogProps) {
	return (
		<DialogLifecycle
			open={props.open}
			target={props}
			onOpenChange={props.onOpenChange}
		>
			{({ target, active, onOpenChange }) => (
				<CreateFileDialogContent
					{...target}
					open={active}
					onOpenChange={onOpenChange}
				/>
			)}
		</DialogLifecycle>
	);
}

function CreateFileDialogContent({
	open,
	onOpenChange,
}: CreateFileDialogProps) {
	const isCurrent = useDialogActivity();
	const { t } = useTranslation("files");
	const createFile = useFileStore((s) => s.createFile);
	const [name, setName] = useState("");
	const [submitting, setSubmitting] = useState(false);
	const submittingRef = useRef(false);

	const handleSubmit = async (e: FormEvent) => {
		e.preventDefault();
		const trimmedName = name.trim();
		if (!trimmedName || submittingRef.current) return;
		submittingRef.current = true;
		setSubmitting(true);
		try {
			await createFile(trimmedName);
			toast.success(t("create_file_success"));
			onOpenChange(false);
		} catch (err) {
			if (isCurrent()) handleApiError(err);
		} finally {
			submittingRef.current = false;
			if (isCurrent()) setSubmitting(false);
		}
	};

	return (
		<Dialog open={open} onOpenChange={onOpenChange}>
			<DialogContent keepMounted>
				<DialogHeader>
					<DialogTitle>{t("create_file")}</DialogTitle>
				</DialogHeader>
				<form onSubmit={handleSubmit} className="space-y-4">
					<Input
						placeholder={t("file_name")}
						value={name}
						onChange={(e) => setName(e.target.value)}
						autoFocus
					/>
					<Button type="submit" className="w-full" disabled={submitting}>
						{t("create_file")}
					</Button>
				</form>
			</DialogContent>
		</Dialog>
	);
}
