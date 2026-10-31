import type { ReactNode } from "react";
import { useTranslation } from "react-i18next";
import {
	Dialog,
	DialogContent,
	DialogDescription,
	DialogFooter,
	DialogHeader,
	DialogTitle,
} from "@/components/ui/dialog";
import { DialogLifecycle } from "@/components/ui/dialog-lifecycle";
import { PreviewAppEditorFields } from "../PreviewAppEditorFields";
import type {
	PreviewAppsEditorApp,
	PreviewAppsEditorConfig,
} from "../previewAppsConfigEditorShared";

interface PreviewAppsEditDialogProps {
	children?: ReactNode;
	app: PreviewAppsEditorApp | null;
	appName: string;
	index: number;
	open: boolean;
	protectedBuiltin: boolean;
	onOpenChange: (open: boolean) => void;
	onOpenUrlTemplateVariables: () => void;
	updateApp: (
		index: number,
		updater: (app: PreviewAppsEditorApp) => PreviewAppsEditorApp,
	) => void;
	updateDraft: (
		updater: (current: PreviewAppsEditorConfig) => PreviewAppsEditorConfig,
	) => void;
}

export function PreviewAppsEditDialog(props: PreviewAppsEditDialogProps) {
	return (
		<DialogLifecycle
			open={props.open}
			identity={props.index}
			target={props}
			onOpenChange={props.onOpenChange}
		>
			{({ target, active, onOpenChange }) => (
				<PreviewAppsEditDialogContent
					{...target}
					open={active}
					onOpenChange={onOpenChange}
				/>
			)}
		</DialogLifecycle>
	);
}

function PreviewAppsEditDialogContent({
	children,
	app,
	appName,
	index,
	onOpenChange,
	onOpenUrlTemplateVariables,
	open,
	protectedBuiltin,
	updateApp,
	updateDraft,
}: PreviewAppsEditDialogProps) {
	const { t } = useTranslation(["admin", "files"]);

	return (
		<Dialog open={open} onOpenChange={onOpenChange}>
			<DialogContent className="max-w-[calc(100%-1.5rem)] sm:max-w-[min(72rem,calc(100vw-2rem))]">
				<DialogHeader>
					<DialogTitle>
						{t("preview_apps_dialog_title", {
							name: appName,
						})}
					</DialogTitle>
					<DialogDescription>{t("preview_apps_dialog_desc")}</DialogDescription>
				</DialogHeader>
				{app ? (
					<div className="max-h-[min(72vh,46rem)] overflow-y-auto py-2 pr-1">
						<PreviewAppEditorFields
							app={app}
							index={index}
							protectedBuiltin={protectedBuiltin}
							t={t}
							updateApp={updateApp}
							updateDraft={updateDraft}
							onOpenUrlTemplateVariables={onOpenUrlTemplateVariables}
						/>
					</div>
				) : null}
				<DialogFooter showCloseButton />
				{children}
			</DialogContent>
		</Dialog>
	);
}
