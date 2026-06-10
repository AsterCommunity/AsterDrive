import { useTranslation } from "react-i18next";
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

interface GeneratedPasswordDialogProps {
	open: boolean;
	password: string | null;
	username: string;
	onCopy: () => void;
	onOpenChange: (open: boolean) => void;
}

export function GeneratedPasswordDialog({
	open,
	password,
	username,
	onCopy,
	onOpenChange,
}: GeneratedPasswordDialogProps) {
	const { t } = useTranslation(["admin", "core"]);

	return (
		<Dialog open={open} onOpenChange={onOpenChange}>
			<DialogContent className="sm:max-w-md">
				<DialogHeader>
					<DialogTitle>{t("generated_password_title")}</DialogTitle>
					<DialogDescription>
						{t("generated_password_dialog_desc", { username })}
					</DialogDescription>
				</DialogHeader>
				<div className="space-y-3 rounded-md border border-primary/30 bg-primary/5 p-3">
					<div className="flex items-start gap-2">
						<Icon
							name="KeyRound"
							className="mt-0.5 size-4 shrink-0 text-primary"
						/>
						<p className="text-sm text-muted-foreground">
							{t("generated_password_desc")}
						</p>
					</div>
					<div className="flex gap-2">
						<Input
							readOnly
							value={password ?? ""}
							className="font-mono text-sm"
							aria-label={t("generated_password")}
						/>
						<Button
							type="button"
							variant="outline"
							size="icon"
							onClick={onCopy}
							disabled={!password}
							aria-label={t("copy_generated_password")}
						>
							<Icon name="Copy" className="size-4" />
						</Button>
					</div>
				</div>
				<DialogFooter>
					<Button type="button" onClick={() => onOpenChange(false)}>
						{t("core:close")}
					</Button>
				</DialogFooter>
			</DialogContent>
		</Dialog>
	);
}
