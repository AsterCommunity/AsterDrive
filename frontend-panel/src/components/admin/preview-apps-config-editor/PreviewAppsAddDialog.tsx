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
import { DialogLifecycle } from "@/components/ui/dialog-lifecycle";

interface PreviewAppsAddDialogProps {
	canBuildWopiDiscoveryConfig: boolean;
	open: boolean;
	onAddEmbedApp: () => void;
	onOpenChange: (open: boolean) => void;
	onOpenWopiDiscovery: () => void;
}

export function PreviewAppsAddDialog(props: PreviewAppsAddDialogProps) {
	return (
		<DialogLifecycle
			open={props.open}
			target={props}
			onOpenChange={props.onOpenChange}
		>
			{({ target, active, onOpenChange }) => (
				<PreviewAppsAddDialogContent
					{...target}
					open={active}
					onOpenChange={onOpenChange}
				/>
			)}
		</DialogLifecycle>
	);
}

function PreviewAppsAddDialogContent({
	canBuildWopiDiscoveryConfig,
	onAddEmbedApp,
	onOpenChange,
	onOpenWopiDiscovery,
	open,
}: PreviewAppsAddDialogProps) {
	const { t } = useTranslation(["admin", "files"]);

	return (
		<Dialog open={open} onOpenChange={onOpenChange}>
			<DialogContent className="max-w-md">
				<DialogHeader>
					<DialogTitle>{t("preview_apps_add_dialog_title")}</DialogTitle>
					<DialogDescription>
						{t("preview_apps_add_dialog_desc")}
					</DialogDescription>
				</DialogHeader>
				<div className="grid gap-3 py-2">
					<Button
						variant="outline"
						className="h-auto w-full min-w-0 items-start justify-start p-4 text-left whitespace-normal"
						onClick={onAddEmbedApp}
					>
						<div className="min-w-0 space-y-1">
							<p className="break-words font-medium">
								{t("preview_apps_add_dialog_embed_title")}
							</p>
							<p className="break-words text-sm text-muted-foreground">
								{t("preview_apps_add_dialog_embed_desc")}
							</p>
						</div>
					</Button>
					{canBuildWopiDiscoveryConfig ? (
						<Button
							variant="outline"
							className="h-auto w-full min-w-0 items-start justify-start p-4 text-left whitespace-normal"
							onClick={onOpenWopiDiscovery}
						>
							<div className="min-w-0 space-y-1">
								<p className="break-words font-medium">
									{t("preview_apps_add_dialog_wopi_title")}
								</p>
								<p className="break-words text-sm text-muted-foreground">
									{t("preview_apps_add_dialog_wopi_desc")}
								</p>
							</div>
						</Button>
					) : null}
				</div>
				<DialogFooter showCloseButton />
			</DialogContent>
		</Dialog>
	);
}
