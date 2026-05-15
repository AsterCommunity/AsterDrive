import { Input as InputPrimitive } from "@base-ui/react/input";
import * as React from "react";

import { cn } from "@/lib/utils";

type SelectionState = {
	direction: "backward" | "forward" | "none";
	end: number;
	start: number;
};

function supportsTextSelection(input: HTMLInputElement) {
	return (
		input.type === "text" ||
		input.type === "search" ||
		input.type === "tel" ||
		input.type === "url" ||
		input.type === "password"
	);
}

function readSelection(input: HTMLInputElement): SelectionState | null {
	if (!supportsTextSelection(input)) {
		return null;
	}

	const start = input.selectionStart;
	const end = input.selectionEnd;
	if (start === null || end === null) {
		return null;
	}

	return {
		direction: input.selectionDirection ?? "none",
		end,
		start,
	};
}

const Input = React.forwardRef<HTMLInputElement, React.ComponentProps<"input">>(
	(
		{
			className,
			onBlur,
			onChange,
			onKeyUp,
			onMouseUp,
			onSelect,
			type,
			...props
		},
		ref,
	) => {
		const inputRef = React.useRef<HTMLInputElement | null>(null);
		const selectionRef = React.useRef<SelectionState | null>(null);
		const assignRef = React.useCallback(
			(node: HTMLInputElement | null) => {
				inputRef.current = node;
				if (typeof ref === "function") {
					ref(node);
				} else if (ref) {
					ref.current = node;
				}
			},
			[ref],
		);
		const captureSelection = React.useCallback(() => {
			const input = inputRef.current;
			if (!input || document.activeElement !== input) {
				return;
			}
			selectionRef.current = readSelection(input);
		}, []);

		React.useLayoutEffect(() => {
			const input = inputRef.current;
			const selection = selectionRef.current;
			if (!input || !selection || document.activeElement !== input) {
				return;
			}
			if (!supportsTextSelection(input)) {
				return;
			}

			const start = Math.min(selection.start, input.value.length);
			const end = Math.min(selection.end, input.value.length);
			input.setSelectionRange(start, end, selection.direction);
		});

		return (
			<InputPrimitive
				ref={assignRef}
				type={type}
				data-slot="input"
				data-theme-surface="control"
				className={cn(
					"h-8 w-full min-w-0 rounded-lg border border-input/80 bg-card/70 px-2.5 py-1 text-base shadow-xs transition-[background-color,border-color,box-shadow] outline-none file:inline-flex file:h-6 file:border-0 file:bg-transparent file:text-sm file:font-medium file:text-foreground placeholder:text-muted-foreground focus-visible:border-ring focus-visible:bg-background focus-visible:ring-3 focus-visible:ring-ring/30 disabled:pointer-events-none disabled:cursor-not-allowed disabled:bg-muted/60 disabled:opacity-60 aria-invalid:border-destructive aria-invalid:ring-3 aria-invalid:ring-destructive/20 md:text-sm dark:bg-input/25 dark:shadow-none dark:focus-visible:bg-input/35 dark:disabled:bg-input/80 dark:aria-invalid:border-destructive/50 dark:aria-invalid:ring-destructive/40",
					className,
				)}
				onChange={(event) => {
					onChange?.(event);
					selectionRef.current = readSelection(event.currentTarget);
				}}
				onSelect={(event) => {
					onSelect?.(event);
					selectionRef.current = readSelection(event.currentTarget);
				}}
				onKeyUp={(event) => {
					onKeyUp?.(event);
					captureSelection();
				}}
				onMouseUp={(event) => {
					onMouseUp?.(event);
					captureSelection();
				}}
				onBlur={(event) => {
					selectionRef.current = null;
					onBlur?.(event);
				}}
				{...props}
			/>
		);
	},
);
Input.displayName = "Input";

export { Input };
