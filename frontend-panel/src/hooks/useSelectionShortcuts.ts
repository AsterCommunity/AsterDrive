import { useEffect, useEffectEvent } from "react";
import {
	isImeComposingKeyEvent,
	shouldIgnoreKeyboardTarget,
} from "@/lib/keyboard";

interface UseSelectionShortcutsOptions {
	selectAll: () => void;
	clearSelection: () => void;
	enabled?: boolean;
}

export { shouldIgnoreKeyboardTarget } from "@/lib/keyboard";

export function useSelectionShortcuts({
	selectAll,
	clearSelection,
	enabled = true,
}: UseSelectionShortcutsOptions) {
	const handleKeyDown = useEffectEvent((e: KeyboardEvent) => {
		if (shouldIgnoreKeyboardTarget(e.target) || isImeComposingKeyEvent(e)) {
			return;
		}

		const mod = e.metaKey || e.ctrlKey;
		if (mod && e.key.toLowerCase() === "a") {
			e.preventDefault();
			selectAll();
		}

		if (e.key === "Escape") {
			clearSelection();
		}
	});

	useEffect(() => {
		if (!enabled) return;

		document.addEventListener("keydown", handleKeyDown);
		return () => document.removeEventListener("keydown", handleKeyDown);
	}, [enabled]);
}
