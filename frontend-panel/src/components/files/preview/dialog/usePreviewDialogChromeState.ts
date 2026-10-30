import { useEffect, useState } from "react";
import { useDialogPresentation } from "@/components/ui/dialog-lifecycle";
import type { OpenWithOption } from "../capabilities/types";
import type { FilePreviewDialogUiAction } from "./filePreviewDialogState";

// Matches Tailwind's md breakpoint boundary.
const MOBILE_PREVIEW_MEDIA_QUERY = "(max-width: 767px)";

function getEmbeddedOptionMode(option: OpenWithOption | null) {
	if (!option) {
		return "new_tab";
	}

	if (option.mode !== "url_template" && option.mode !== "wopi") {
		return "iframe";
	}

	return option.config?.mode === "new_tab" ? "new_tab" : "iframe";
}

function useMediaQuery(query: string) {
	const [matches, setMatches] = useState(() =>
		typeof window.matchMedia === "function"
			? window.matchMedia(query).matches
			: false,
	);

	useEffect(() => {
		if (typeof window.matchMedia !== "function") {
			setMatches(false);
			return;
		}

		const mediaQuery = window.matchMedia(query);
		setMatches(mediaQuery.matches);
		const handleChange = () => {
			setMatches(mediaQuery.matches);
		};
		mediaQuery.addEventListener("change", handleChange);
		return () => {
			mediaQuery.removeEventListener("change", handleChange);
		};
	}, [query]);

	return matches;
}

interface UsePreviewDialogChromeStateOptions {
	activeOption: OpenWithOption | null;
	dispatch: React.Dispatch<FilePreviewDialogUiAction>;
	hasManualExpanded: boolean;
	isDialogAnimationEnabled: boolean;
	isExpanded: boolean;
	open: boolean;
	showOpenMethodChooser: boolean;
}

export function usePreviewDialogChromeState({
	activeOption,
	dispatch,
	hasManualExpanded,
	isDialogAnimationEnabled,
	isExpanded: stateExpanded,
	open,
	showOpenMethodChooser,
}: UsePreviewDialogChromeStateOptions) {
	const phase = useDialogPresentation()?.phase;
	const isMobilePreviewViewport = useMediaQuery(MOBILE_PREVIEW_MEDIA_QUERY);
	const usesInnerScroll =
		activeOption?.mode === "pdf" ||
		activeOption?.mode === "table" ||
		((activeOption?.mode === "url_template" || activeOption?.mode === "wopi") &&
			getEmbeddedOptionMode(activeOption) !== "new_tab");
	const fillsViewportHeight =
		activeOption?.mode === "code" ||
		activeOption?.mode === "formatted" ||
		activeOption?.mode === "markdown" ||
		activeOption?.mode === "archive" ||
		activeOption?.mode === "pdf" ||
		activeOption?.mode === "table" ||
		((activeOption?.mode === "url_template" || activeOption?.mode === "wopi") &&
			getEmbeddedOptionMode(activeOption) !== "new_tab");
	const isImagePreview = activeOption?.mode === "image";
	const isExpanded =
		isMobilePreviewViewport ||
		(isImagePreview
			? hasManualExpanded
				? stateExpanded
				: true
			: stateExpanded);

	useEffect(() => {
		if (!open || showOpenMethodChooser || !isDialogAnimationEnabled) {
			return;
		}

		if (phase === "open") dispatch({ type: "disableAnimation" });
	}, [dispatch, isDialogAnimationEnabled, open, phase, showOpenMethodChooser]);

	const dialogContentClassName = showOpenMethodChooser
		? "flex max-h-[min(90vh,calc(100vh-2rem))] w-[min(96vw,32rem)] max-w-[min(96vw,32rem)] flex-col gap-0 overflow-hidden p-0 sm:max-w-[min(96vw,32rem)]"
		: [
				"flex max-h-[90vh] w-[min(96vw,1200px)] max-w-[min(96vw,1200px)] flex-col gap-0 overflow-hidden p-0 sm:max-w-[min(96vw,1200px)]",
				(fillsViewportHeight || isImagePreview || isExpanded) && "h-[90vh]",
				isImagePreview &&
					"group/image-preview border-zinc-900 bg-zinc-950 shadow-black/35 duration-200 data-open:zoom-in-95 data-closed:zoom-out-95",
				isExpanded &&
					"top-0 left-0 h-screen w-screen max-h-screen max-w-none translate-x-0 translate-y-0 rounded-none sm:max-w-none",
			]
				.filter(Boolean)
				.join(" ");
	const dialogOverlayClassName = isImagePreview
		? "bg-zinc-950/88 duration-200 supports-backdrop-filter:backdrop-blur-xs dark:bg-zinc-950/88"
		: undefined;

	return {
		dialogContentClassName,
		dialogOverlayClassName,
		fillsViewportHeight,
		isExpanded,
		isImagePreview,
		usesInnerScroll,
	};
}
