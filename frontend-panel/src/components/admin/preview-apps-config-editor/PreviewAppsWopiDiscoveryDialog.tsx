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
import { Input } from "@/components/ui/input";

interface PreviewAppsWopiDiscoveryDialogProps {
	building: boolean;
	open: boolean;
	url: string;
	onBuild: () => void;
	onOpenChange: (open: boolean) => void;
	onUrlChange: (url: string) => void;
}

export function PreviewAppsWopiDiscoveryDialog(
	props: PreviewAppsWopiDiscoveryDialogProps,
) {
	return (
		<DialogLifecycle
			open={props.open}
			target={props}
			onOpenChange={props.onOpenChange}
		>
			{({ target, active, onOpenChange }) => (
				<PreviewAppsWopiDiscoveryDialogContent
					{...target}
					open={active}
					onOpenChange={onOpenChange}
				/>
			)}
		</DialogLifecycle>
	);
}

function PreviewAppsWopiDiscoveryDialogContent({
	building,
	onBuild,
	onOpenChange,
	onUrlChange,
	open,
	url,
}: PreviewAppsWopiDiscoveryDialogProps) {
	const { t } = useTranslation(["admin", "files"]);

	return (
		<Dialog open={open} onOpenChange={onOpenChange}>
			<DialogContent className="max-w-md">
				<DialogHeader>
					<DialogTitle>
						{t("preview_apps_wopi_discovery_dialog_title")}
					</DialogTitle>
					<DialogDescription>
						{t("preview_apps_wopi_discovery_dialog_desc")}
					</DialogDescription>
				</DialogHeader>
				<div className="space-y-2 py-2">
					<p className="text-xs font-medium text-muted-foreground">
						{t("preview_apps_wopi_discovery_dialog_label")}
					</p>
					<Input
						aria-label={t("preview_apps_wopi_discovery_dialog_label")}
						placeholder={t("preview_apps_wopi_discovery_dialog_placeholder")}
						value={url}
						onChange={(event) => onUrlChange(event.target.value)}
					/>
				</div>
				<DialogFooter>
					<Button variant="outline" onClick={() => onOpenChange(false)}>
						{t("core:cancel")}
					</Button>
					<Button
						disabled={building || url.trim().length === 0}
						onClick={onBuild}
					>
						{building
							? t("preview_apps_wopi_discovery_dialog_loading")
							: t("preview_apps_wopi_discovery_dialog_submit")}
					</Button>
				</DialogFooter>
			</DialogContent>
		</Dialog>
	);
}
