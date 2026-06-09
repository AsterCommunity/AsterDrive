import { useCallback, useEffect, useRef, useState } from "react";
import { useTranslation } from "react-i18next";
import { Button } from "@/components/ui/button";
import { Icon } from "@/components/ui/icon";
import { useFileEditorSession } from "@/hooks/useFileEditorSession";
import { useTextContent } from "@/hooks/useTextContent";
import {
	CodePreviewEditor,
	type CodePreviewEditorMountHandler,
} from "./CodePreviewEditor";
import { getEditorLanguage } from "./file-capabilities";
import { PreviewError } from "./PreviewError";
import { PreviewLoadingState } from "./PreviewLoadingState";
import type { PreviewableFileLike } from "./types";

interface TextCodePreviewProps {
	file: PreviewableFileLike & { id: number };
	modeLabel?: string;
	path: string;
	onFileUpdated?: () => void;
	onDirtyChange?: (dirty: boolean) => void;
	editable?: boolean;
}

function useIsDark() {
	const [dark, setDark] = useState(() =>
		document.documentElement.classList.contains("dark"),
	);

	useEffect(() => {
		const observer = new MutationObserver(() => {
			setDark(document.documentElement.classList.contains("dark"));
		});
		observer.observe(document.documentElement, {
			attributes: true,
			attributeFilter: ["class"],
		});
		return () => observer.disconnect();
	}, []);

	return dark;
}

export function TextCodePreview({
	file,
	modeLabel,
	path,
	onFileUpdated,
	onDirtyChange,
	editable = true,
}: TextCodePreviewProps) {
	const { t } = useTranslation(["core", "files"]);
	const isDark = useIsDark();
	const { content, etag, loading, error, reload } = useTextContent(path);
	const {
		editing,
		dirty,
		editContent,
		saving,
		setEditContent,
		startEditing,
		cancelEditing,
		save,
	} = useFileEditorSession({
		fileId: file.id,
		initialContent: content ?? "",
		etag,
		onSaved: async () => {
			await reload();
			await onFileUpdated?.();
		},
		onConflict: () => reload(),
		messages: {
			saved: t("files:file_saved"),
			editedByOthers: t("files:edited_by_others"),
		},
	});
	const saveRef = useRef(save);

	useEffect(() => {
		saveRef.current = save;
	}, [save]);

	useEffect(() => {
		onDirtyChange?.(dirty);
	}, [dirty, onDirtyChange]);

	const handleEditorMount = useCallback<CodePreviewEditorMountHandler>(
		(editor, shortcutApi) => {
			editor.addCommand(
				shortcutApi.KeyMod.CtrlCmd | shortcutApi.KeyCode.KeyS,
				() => {
					saveRef.current();
				},
			);
		},
		[],
	);

	if (loading) {
		return (
			<PreviewLoadingState
				text={t("files:loading_preview")}
				className="h-full"
			/>
		);
	}

	if (error || content === null) {
		return <PreviewError onRetry={() => void reload()} />;
	}

	const language = getEditorLanguage(file);
	const modeText = (modeLabel?.trim() ?? "") || t("files:open_with_code");
	const statusText = dirty ? t("files:unsaved_changes") : t("core:active");

	return (
		<div className="flex h-full min-h-0 w-full min-w-0 flex-col overflow-hidden rounded-xl border border-border/70 bg-card shadow-xs dark:shadow-none">
			<div className="flex flex-wrap items-center gap-3 border-b border-border/60 bg-muted/25 px-4 py-2 dark:bg-muted/15">
				<div className="flex min-w-0 flex-1 items-center gap-3 text-xs text-muted-foreground">
					<Icon name="FileCode" className="size-4 text-muted-foreground" />
					<span className="font-medium text-foreground">{language}</span>
					<span>·</span>
					<span>{modeText}</span>
					<span>·</span>
					<span>{statusText}</span>
					{editing ? (
						<>
							<span>·</span>
							<span>{t("files:save_shortcut_hint")}</span>
						</>
					) : null}
				</div>
				<div className="ml-auto flex items-center gap-2">
					{!editing ? (
						editable ? (
							<Button variant="outline" size="sm" onClick={startEditing}>
								<Icon name="PencilSimple" className="mr-1 size-3.5" />
								{t("core:edit")}
							</Button>
						) : null
					) : (
						<>
							<Button
								variant="default"
								size="sm"
								onClick={save}
								disabled={saving}
							>
								<Icon name="FloppyDisk" className="mr-1 size-3.5" />
								{saving ? t("files:saving") : t("core:save")}
							</Button>
							<Button variant="outline" size="sm" onClick={cancelEditing}>
								<Icon name="Undo" className="mr-1 size-3.5" />
								{t("core:cancel")}
							</Button>
						</>
					)}
				</div>
			</div>
			<div className="min-h-0 w-full min-w-0 flex-1 overflow-hidden bg-background/80 dark:bg-background/25">
				<CodePreviewEditor
					key={path}
					language={language}
					theme={isDark ? "vs-dark" : "vs"}
					value={editing ? editContent : content}
					onChange={(value) => setEditContent(value ?? "")}
					onMount={handleEditorMount}
					options={{
						readOnly: !editing,
						minimap: { enabled: true },
						wordWrap: "off",
						fontSize: 13,
						lineNumbers: "on",
						scrollBeyondLastLine: false,
						renderLineHighlight: editing ? "line" : "none",
						domReadOnly: !editing,
						padding: { top: 12 },
					}}
				/>
			</div>
		</div>
	);
}
