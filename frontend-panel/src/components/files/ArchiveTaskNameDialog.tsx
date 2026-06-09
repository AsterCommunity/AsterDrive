import type { FormEvent } from "react";
import { useEffect, useState } from "react";
import { useTranslation } from "react-i18next";
import { Button } from "@/components/ui/button";
import {
	Dialog,
	DialogContent,
	DialogHeader,
	DialogTitle,
} from "@/components/ui/dialog";
import { Input } from "@/components/ui/input";
import { Label } from "@/components/ui/label";
import {
	Select,
	SelectContent,
	SelectItem,
	SelectTrigger,
	SelectValue,
} from "@/components/ui/select";
import { handleApiError } from "@/hooks/useApiError";
import {
	archiveFilenameEncodingOptions,
	isArchiveFilenameEncoding,
} from "@/lib/archiveFilenameEncoding";
import type { ArchiveFilenameEncoding } from "@/types/api";

interface ArchiveTaskNameDialogProps {
	open: boolean;
	onOpenChange: (open: boolean) => void;
	onOpenChangeComplete?: (open: boolean) => void;
	mode: "compress" | "extract";
	initialName: string;
	onSubmit: (
		name: string | undefined,
		filenameEncoding?: ArchiveFilenameEncoding,
	) => Promise<void>;
}

export function ArchiveTaskNameDialog({
	open,
	onOpenChange,
	onOpenChangeComplete,
	mode,
	initialName,
	onSubmit,
}: ArchiveTaskNameDialogProps) {
	const { t } = useTranslation(["core", "tasks"]);
	const [name, setName] = useState("");
	const [filenameEncoding, setFilenameEncoding] =
		useState<ArchiveFilenameEncoding>("auto");
	const [submitting, setSubmitting] = useState(false);

	useEffect(() => {
		if (open) {
			setName(initialName);
			setFilenameEncoding("auto");
		}
	}, [initialName, open]);

	const handleSubmit = async (event: FormEvent) => {
		event.preventDefault();
		try {
			setSubmitting(true);
			const trimmed = name.trim();
			if (mode === "extract") {
				await onSubmit(trimmed ? trimmed : undefined, filenameEncoding);
			} else {
				await onSubmit(trimmed ? trimmed : undefined);
			}
			onOpenChange(false);
		} catch (error) {
			handleApiError(error);
		} finally {
			setSubmitting(false);
		}
	};

	const title =
		mode === "compress"
			? t("tasks:archive_compress_dialog_title")
			: t("tasks:archive_extract_dialog_title");
	const placeholder =
		mode === "compress"
			? t("tasks:archive_compress_name_placeholder")
			: t("tasks:archive_extract_name_placeholder");
	const hint =
		mode === "compress"
			? t("tasks:archive_compress_dialog_hint")
			: t("tasks:archive_extract_dialog_hint");
	const submitLabel =
		mode === "compress"
			? t("tasks:archive_compress_submit")
			: t("tasks:archive_extract_submit");

	return (
		<Dialog
			open={open}
			onOpenChange={(nextOpen) => {
				if (!submitting) {
					onOpenChange(nextOpen);
				}
			}}
			onOpenChangeComplete={onOpenChangeComplete}
		>
			<DialogContent keepMounted>
				<DialogHeader>
					<DialogTitle>{title}</DialogTitle>
				</DialogHeader>
				<form onSubmit={handleSubmit} className="space-y-4">
					<Input
						placeholder={placeholder}
						value={name}
						onChange={(event) => setName(event.target.value)}
						autoFocus
						onFocus={(event) => {
							if (
								mode === "compress" &&
								event.target.value.toLowerCase().endsWith(".zip") &&
								event.target.value.length > 4
							) {
								event.target.setSelectionRange(
									0,
									event.target.value.length - 4,
								);
								return;
							}
							event.target.select();
						}}
					/>
					{mode === "extract" ? (
						<div className="space-y-2">
							<Label htmlFor="archive-filename-encoding">
								{t("tasks:archive_filename_encoding_label")}
							</Label>
							<Select
								value={filenameEncoding}
								onValueChange={(value) => {
									if (isArchiveFilenameEncoding(value)) {
										setFilenameEncoding(value);
									}
								}}
								disabled={submitting}
							>
								<SelectTrigger id="archive-filename-encoding">
									<SelectValue />
								</SelectTrigger>
								<SelectContent>
									{archiveFilenameEncodingOptions.map((value) => (
										<SelectItem key={value} value={value}>
											{t(`tasks:archive_filename_encoding_${value}`)}
										</SelectItem>
									))}
								</SelectContent>
							</Select>
						</div>
					) : null}
					<p className="text-sm text-muted-foreground">{hint}</p>
					<div className="flex gap-2">
						<Button
							type="button"
							variant="outline"
							className="flex-1"
							onClick={() => onOpenChange(false)}
							disabled={submitting}
						>
							{t("core:cancel")}
						</Button>
						<Button type="submit" className="flex-1" disabled={submitting}>
							{submitLabel}
						</Button>
					</div>
				</form>
			</DialogContent>
		</Dialog>
	);
}
