import { Input as InputPrimitive } from "@base-ui/react/input";
import {
	type ComponentProps,
	type Ref,
	useCallback,
	useLayoutEffect,
	useRef,
} from "react";
import {
	textControlBaseClasses,
	textControlStateClasses,
} from "@/components/ui/text-control-styles";
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

function Input({
	className,
	onBlur,
	onChange,
	onKeyUp,
	onMouseUp,
	onSelect,
	ref,
	type,
	...props
}: ComponentProps<"input"> & {
	ref?: Ref<HTMLInputElement>;
}) {
	const inputRef = useRef<HTMLInputElement | null>(null);
	const selectionRef = useRef<SelectionState | null>(null);
	const assignRef = useCallback(
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
	const captureSelection = useCallback(() => {
		const input = inputRef.current;
		if (!input || document.activeElement !== input) {
			return;
		}
		selectionRef.current = readSelection(input);
	}, []);

	useLayoutEffect(() => {
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
				textControlBaseClasses,
				textControlStateClasses,
				"h-8 px-2.5 py-1 file:inline-flex file:h-6 file:border-0 file:bg-transparent file:text-sm file:font-medium file:text-foreground",
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
}

export { Input };
