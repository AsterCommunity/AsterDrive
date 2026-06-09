import { useCallback, useEffect, useRef, useState } from "react";
import { useTranslation } from "react-i18next";
import { useFileEditorSession } from "@/hooks/useFileEditorSession";
import { useTextContent } from "@/hooks/useTextContent";
import {
	CodePreviewEditor,
	type CodePreviewEditorMountHandler,
} from "./CodePreviewEditor";
import { getEditorLanguage } from "./file-capabilities";
import { PreviewError } from "./PreviewError";
import { PreviewLoadingState } from "./PreviewLoadingState";
import { PreviewSurface, PreviewSurfaceContent } from "./PreviewSurface";
import { TextCodePreviewToolbar } from "./TextCodePreviewToolbar";
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

	const language = getEditorLanguage(file);

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

	return (
		<PreviewSurface>
			<TextCodePreviewToolbar
				cancelEditing={cancelEditing}
				dirty={dirty}
				editable={editable}
				editing={editing}
				language={language}
				modeLabel={modeLabel}
				save={save}
				saving={saving}
				startEditing={startEditing}
			/>
			<PreviewSurfaceContent>
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
			</PreviewSurfaceContent>
		</PreviewSurface>
	);
}
