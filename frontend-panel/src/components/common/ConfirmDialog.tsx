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
import { DialogLifecycle } from "@/components/ui/dialog-lifecycle";

export interface ConfirmDialogProps {
	open: boolean;
	onOpenChange: (open: boolean) => void;
	title: string;
	description?: string;
	confirmLabel?: string;
	onConfirm: () => void;
	variant?: "default" | "destructive";
}

export function ConfirmDialog(props: ConfirmDialogProps) {
	return (
		<DialogLifecycle
			open={props.open}
			target={props}
			onOpenChange={props.onOpenChange}
		>
			{({ target, active, onOpenChange }) => (
				<ConfirmDialogContent
					{...target}
					open={active}
					onOpenChange={onOpenChange}
				/>
			)}
		</DialogLifecycle>
	);
}

function ConfirmDialogContent({
	open,
	onOpenChange,
	title,
	description,
	confirmLabel,
	onConfirm,
	variant = "default",
}: ConfirmDialogProps) {
	const { t } = useTranslation();

	const handleConfirm = () => {
		onOpenChange(false);
		onConfirm();
	};

	return (
		<AlertDialog open={open} onOpenChange={onOpenChange}>
			<AlertDialogContent keepMounted>
				<AlertDialogHeader>
					<AlertDialogTitle>{title}</AlertDialogTitle>
					{description && (
						<AlertDialogDescription>{description}</AlertDialogDescription>
					)}
				</AlertDialogHeader>
				<AlertDialogFooter>
					<AlertDialogCancel>{t("cancel")}</AlertDialogCancel>
					<AlertDialogAction
						onClick={handleConfirm}
						className={
							variant === "destructive"
								? "bg-destructive text-white hover:bg-destructive/90"
								: ""
						}
					>
						{confirmLabel || t("confirm")}
					</AlertDialogAction>
				</AlertDialogFooter>
			</AlertDialogContent>
		</AlertDialog>
	);
}
