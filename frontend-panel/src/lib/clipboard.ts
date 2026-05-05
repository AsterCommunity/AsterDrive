export async function writeTextToClipboard(value: string): Promise<void> {
	const clipboard = navigator.clipboard;
	if (clipboard?.writeText) {
		try {
			await clipboard.writeText(value);
			return;
		} catch {
			// Fall back for browsers that expose Clipboard API but reject it because
			// of focus, permission, or secure-context checks.
		}
	}

	writeTextWithLegacyCommand(value);
}

function writeTextWithLegacyCommand(value: string): void {
	if (typeof document.execCommand !== "function") {
		throw new Error("Clipboard copy is not supported");
	}

	const activeElement = document.activeElement;
	const textarea = document.createElement("textarea");
	textarea.value = value;
	textarea.readOnly = true;
	textarea.style.position = "fixed";
	textarea.style.top = "0";
	textarea.style.left = "0";
	textarea.style.width = "1px";
	textarea.style.height = "1px";
	textarea.style.opacity = "0";
	textarea.style.pointerEvents = "none";

	document.body.appendChild(textarea);

	try {
		textarea.focus({ preventScroll: true });
		textarea.select();
		textarea.setSelectionRange(0, value.length);

		if (!document.execCommand("copy")) {
			throw new Error("Clipboard copy failed");
		}
	} finally {
		document.body.removeChild(textarea);
		if (activeElement instanceof HTMLElement) {
			activeElement.focus({ preventScroll: true });
		}
	}
}
