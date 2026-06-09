import { memo, useMemo } from "react";
import { useTranslation } from "react-i18next";
import { Button } from "@/components/ui/button";
import { Icon } from "@/components/ui/icon";
import { PreviewSurfaceToolbar } from "./PreviewSurface";

interface TextCodePreviewToolbarProps {
	cancelEditing: () => void;
	dirty: boolean;
	editable: boolean;
	editing: boolean;
	language: string;
	modeLabel?: string;
	save: () => void | Promise<void>;
	saving: boolean;
	startEditing: () => void;
}

export const TextCodePreviewToolbar = memo(function TextCodePreviewToolbar({
	cancelEditing,
	dirty,
	editable,
	editing,
	language,
	modeLabel,
	save,
	saving,
	startEditing,
}: TextCodePreviewToolbarProps) {
	const { t } = useTranslation(["core", "files"]);
	const modeText = (modeLabel?.trim() ?? "") || t("files:open_with_code");
	const statusText = dirty ? t("files:unsaved_changes") : t("core:active");
	const toolbarMeta = useMemo(
		() => (
			<span className="inline-flex min-w-0 items-center gap-2">
				<span className="truncate">{modeText}</span>
				<span className="shrink-0 text-muted-foreground/60">·</span>
				<span className="shrink-0">{statusText}</span>
				{editing ? (
					<>
						<span className="shrink-0 text-muted-foreground/60">·</span>
						<span className="truncate">{t("files:save_shortcut_hint")}</span>
					</>
				) : null}
			</span>
		),
		[editing, modeText, statusText, t],
	);
	const toolbarActions = useMemo(
		() =>
			!editing ? (
				editable ? (
					<Button variant="outline" size="sm" onClick={startEditing}>
						<Icon name="PencilSimple" className="mr-1 size-3.5" />
						{t("core:edit")}
					</Button>
				) : null
			) : (
				<>
					<Button variant="default" size="sm" onClick={save} disabled={saving}>
						<Icon name="FloppyDisk" className="mr-1 size-3.5" />
						{saving ? t("files:saving") : t("core:save")}
					</Button>
					<Button variant="outline" size="sm" onClick={cancelEditing}>
						<Icon name="Undo" className="mr-1 size-3.5" />
						{t("core:cancel")}
					</Button>
				</>
			),
		[cancelEditing, editable, editing, save, saving, startEditing, t],
	);

	return (
		<PreviewSurfaceToolbar
			icon="FileCode"
			label={language}
			meta={toolbarMeta}
			actions={toolbarActions}
		/>
	);
});
