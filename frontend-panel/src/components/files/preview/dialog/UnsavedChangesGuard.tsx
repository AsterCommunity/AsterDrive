import { useTranslation } from "react-i18next";
import { Button } from "@/components/ui/button";
import {
	DialogLifecycle,
	useDialogMotion,
} from "@/components/ui/dialog-lifecycle";
import { cn } from "@/lib/utils";

interface UnsavedChangesGuardProps {
	open: boolean;
	onOpenChange: (open: boolean) => void;
	onConfirm: () => void;
}

export function UnsavedChangesGuard(props: UnsavedChangesGuardProps) {
	return (
		<DialogLifecycle
			open={props.open}
			target={props}
			onOpenChange={props.onOpenChange}
		>
			{({ target, active, onOpenChange }) => (
				<UnsavedChangesContent
					{...target}
					open={active}
					onOpenChange={onOpenChange}
				/>
			)}
		</DialogLifecycle>
	);
}

function UnsavedChangesContent({
	open,
	onOpenChange,
	onConfirm,
}: UnsavedChangesGuardProps) {
	const { t } = useTranslation(["core", "files"]);
	const { ref } = useDialogMotion(open);

	return (
		<div
			ref={ref}
			data-testid="unsaved-changes-guard"
			aria-hidden={!open}
			className={cn(
				"fixed inset-x-3 bottom-3 z-(--z-alert-dialog) mx-auto flex max-w-xl origin-bottom flex-col gap-3 rounded-xl border border-destructive/30 bg-popover p-4 text-sm shadow-2xl shadow-black/15 ring-1 ring-foreground/5 motion-reduce:animate-none sm:bottom-5 sm:flex-row sm:items-center sm:justify-between dark:shadow-none",
				open
					? "duration-[180ms] animate-in fade-in zoom-in-95 slide-in-from-bottom-4 ease-out"
					: "pointer-events-none duration-[140ms] animate-out fade-out zoom-out-95 slide-out-to-bottom-4 ease-in",
			)}
		>
			<div>
				<p className="font-medium text-foreground">{t("are_you_sure")}</p>
				<p className="mt-1 text-muted-foreground">
					{t("files:unsaved_confirm_desc")}
				</p>
			</div>
			<div className="flex shrink-0 items-center justify-end gap-2">
				<Button
					type="button"
					variant="outline"
					onClick={() => onOpenChange(false)}
				>
					{t("cancel")}
				</Button>
				<Button type="button" variant="destructive" onClick={onConfirm}>
					{t("files:discard_changes")}
				</Button>
			</div>
		</div>
	);
}
