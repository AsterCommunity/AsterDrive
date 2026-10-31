import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import { toast } from "sonner";
import {
	useDialogActivity,
	useDialogPresentation,
} from "@/components/ui/dialog-lifecycle";
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
	const active = useDialogPresentation()?.active ?? true;
	const isCurrent = useDialogActivity(fileId);
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
		const release = () => {
			if (!editingRef.current) return;
			editingRef.current = false;
			void fileService.setFileLock(fileId, false).catch(() => {});
		};
		if (!active) release();
		return release;
	}, [fileId, active]);

	useEffect(() => {
		if (!active || !editing) return;
		const handler = (e: BeforeUnloadEvent) => {
			e.preventDefault();
		};
		window.addEventListener("beforeunload", handler);
		return () => window.removeEventListener("beforeunload", handler);
	}, [editing, active]);

	const dirty = useMemo(
		() => editing && editContent !== initialContent,
		[editContent, editing, initialContent],
	);

	const startEditing = useCallback(async () => {
		try {
			await fileService.setFileLock(fileId, true);
			if (!isCurrent()) {
				await fileService.setFileLock(fileId, false);
				return;
			}
			setEditing(true);
		} catch (error) {
			if (isCurrent()) handleApiError(error);
		}
	}, [fileId, isCurrent]);

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
			if (isCurrent()) setEditing(false);
			toast.success(messages.saved);
			await onSaved?.();
			try {
				await fileService.setFileLock(fileId, false);
			} catch {
				// 解锁失败不阻塞
			}
		} catch (error: unknown) {
			const status = (error as { status?: number })?.status;
			if (!isCurrent()) return;
			if (status === 412) {
				toast.error(messages.editedByOthers);
				onConflict?.();
			} else {
				if (isCurrent()) handleApiError(error);
			}
		} finally {
			if (isCurrent()) setSaving(false);
		}
	}, [editContent, etag, fileId, messages, onConflict, onSaved, isCurrent]);

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
