import { useRef } from "react";
import { useTranslation } from "react-i18next";
import {
	AlertDialog,
	AlertDialogAction,
	AlertDialogCancel,
	AlertDialogContent,
	AlertDialogDescription,
	AlertDialogFooter,
	AlertDialogHeader,
	AlertDialogTitle,
} from "@/components/ui/alert-dialog";

export interface ConfirmDialogProps {
	open: boolean;
	onOpenChange: (open: boolean) => void;
	title: string;
	description?: string;
	confirmLabel?: string;
	onConfirm: () => void;
	variant?: "default" | "destructive";
}

export function ConfirmDialog({
	open,
	onOpenChange,
	title,
	description,
	confirmLabel,
	onConfirm,
	variant = "default",
}: ConfirmDialogProps) {
	const { t } = useTranslation();
	const contentRef = useRef({
		confirmLabel,
		description,
		title,
		variant,
	});

	if (open) {
		contentRef.current = {
			confirmLabel,
			description,
			title,
			variant,
		};
	}

	const content = contentRef.current;

	const handleConfirm = () => {
		onOpenChange(false);
		onConfirm();
	};

	return (
		<AlertDialog open={open} onOpenChange={onOpenChange}>
			<AlertDialogContent keepMounted>
				<AlertDialogHeader>
					<AlertDialogTitle>{content.title}</AlertDialogTitle>
					{content.description && (
						<AlertDialogDescription>
							{content.description}
						</AlertDialogDescription>
					)}
				</AlertDialogHeader>
				<AlertDialogFooter>
					<AlertDialogCancel>{t("cancel")}</AlertDialogCancel>
					<AlertDialogAction
						onClick={handleConfirm}
						className={
							content.variant === "destructive"
								? "bg-destructive text-white hover:bg-destructive/90"
								: ""
						}
					>
						{content.confirmLabel || t("confirm")}
					</AlertDialogAction>
				</AlertDialogFooter>
			</AlertDialogContent>
		</AlertDialog>
	);
}
