import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import { toast } from "sonner";
import { handleApiError } from "@/hooks/useApiError";
import { fileService } from "@/services/fileService";

interface UseFileEditorSessionOptions {
	fileId: number;
	initialContent: string;
	etag: string | null;
	onSaved?: () => void | Promise<void>;
	onConflict?: () => void;
	messages: {
		saved: string;
		editedByOthers: string;
	};
}

export function useFileEditorSession({
	fileId,
	initialContent,
	etag,
	onSaved,
	onConflict,
	messages,
}: UseFileEditorSessionOptions) {
	const [editing, setEditing] = useState(false);
	const [editContent, setEditContent] = useState(initialContent);
	const [saving, setSaving] = useState(false);
	const editingRef = useRef(false);

	useEffect(() => {
		if (!editing) {
			setEditContent(initialContent);
		}
	}, [initialContent, editing]);

	useEffect(() => {
		editingRef.current = editing;
	}, [editing]);

	useEffect(() => {
		const editingState = editingRef;
		return () => {
			if (editingState.current) {
				fileService.setFileLock(fileId, false).catch(() => {});
			}
		};
	}, [fileId]);

	useEffect(() => {
		if (!editing) return;
		const handler = (e: BeforeUnloadEvent) => {
			e.preventDefault();
		};
		window.addEventListener("beforeunload", handler);
		return () => window.removeEventListener("beforeunload", handler);
	}, [editing]);

	const dirty = useMemo(
		() => editing && editContent !== initialContent,
		[editContent, editing, initialContent],
	);

	const startEditing = useCallback(async () => {
		try {
			await fileService.setFileLock(fileId, true);
			setEditing(true);
		} catch (error) {
			handleApiError(error);
		}
	}, [fileId]);

	const cancelEditing = useCallback(async () => {
		setEditing(false);
		setEditContent(initialContent);
		try {
			await fileService.setFileLock(fileId, false);
		} catch {
			// 解锁失败不阻塞
		}
	}, [fileId, initialContent]);

	const save = useCallback(async () => {
		setSaving(true);
		try {
			await fileService.updateContent(fileId, editContent, etag ?? undefined);
			setEditing(false);
			toast.success(messages.saved);
			await onSaved?.();
			try {
				await fileService.setFileLock(fileId, false);
			} catch {
				// 解锁失败不阻塞
			}
		} catch (error: unknown) {
			const status = (error as { status?: number })?.status;
			if (status === 412) {
				toast.error(messages.editedByOthers);
				onConflict?.();
			} else {
				handleApiError(error);
			}
		} finally {
			setSaving(false);
		}
	}, [editContent, etag, fileId, messages, onConflict, onSaved]);

	return {
		editing,
		dirty,
		editContent,
		saving,
		setEditContent,
		startEditing,
		cancelEditing,
		save,
	};
}
