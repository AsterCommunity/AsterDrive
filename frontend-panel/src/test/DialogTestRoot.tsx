import type { ReactNode } from "react";
import {
	DialogCompletionBoundary,
	useDialogExitComplete,
} from "@/components/ui/dialog-lifecycle";

/** Explicit exit notification: jsdom must never pretend close means unmount. */
export function DialogTestRoot({
	children,
	open,
	onOpenChange,
	onOpenChangeComplete,
	closeLabel = "dialog-close",
	exitLabel = "complete-close",
	hideClosed = false,
}: {
	children: ReactNode;
	open: boolean;
	onOpenChange?: (open: boolean) => void;
	onOpenChangeComplete?: (open: boolean) => void;
	closeLabel?: string;
	exitLabel?: string;
	hideClosed?: boolean;
}) {
	const complete = useDialogExitComplete();
	return (
		<div data-testid="dialog" data-open={open} hidden={hideClosed && !open}>
			<DialogCompletionBoundary>{children}</DialogCompletionBoundary>
			<button type="button" onClick={() => onOpenChange?.(false)}>
				{closeLabel}
			</button>
			<button
				type="button"
				onClick={() => {
					onOpenChangeComplete?.(false);
					complete?.(false);
				}}
			>
				{exitLabel}
			</button>
		</div>
	);
}
