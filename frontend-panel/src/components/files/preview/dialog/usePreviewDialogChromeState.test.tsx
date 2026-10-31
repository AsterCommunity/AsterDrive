import { act, renderHook } from "@testing-library/react";
import { beforeEach, describe, expect, it, vi } from "vitest";
import { DialogLifecycle } from "@/components/ui/dialog-lifecycle";
import type { OpenWithOption } from "../capabilities/types";
import { usePreviewDialogChromeState } from "./usePreviewDialogChromeState";

const imageOption: OpenWithOption = {
	icon: "Image",
	key: "builtin.image",
	labelKey: "open_with_image",
	mode: "image",
};
const pdfOption: OpenWithOption = {
	icon: "FileText",
	key: "builtin.pdf",
	labelKey: "open_with_pdf",
	mode: "pdf",
};
const externalViewerOption: OpenWithOption = {
	config: { mode: "new_tab" },
	icon: "Globe",
	key: "external.viewer",
	labelKey: "open_with_external",
	mode: "url_template",
};
const embeddedViewerOption: OpenWithOption = {
	config: { mode: "iframe" },
	icon: "Globe",
	key: "embedded.viewer",
	labelKey: "open_with_embedded",
	mode: "url_template",
};

function installMatchMedia(matches = false) {
	vi.stubGlobal(
		"matchMedia",
		vi.fn((query: string) => ({
			matches,
			media: query,
			onchange: null,
			addEventListener: vi.fn(),
			removeEventListener: vi.fn(),
			addListener: vi.fn(),
			removeListener: vi.fn(),
			dispatchEvent: vi.fn(),
		})),
	);
}

let finishEntrance = (_open: boolean) => {};

function renderChrome(
	overrides: Partial<Parameters<typeof usePreviewDialogChromeState>[0]> = {},
) {
	const dispatch = vi.fn();
	const props: Parameters<typeof usePreviewDialogChromeState>[0] = {
		activeOption: pdfOption,
		dispatch,
		hasManualExpanded: false,
		isDialogAnimationEnabled: true,
		isExpanded: false,
		open: true,
		showOpenMethodChooser: false,
		...overrides,
	};
	const hook = renderHook(
		(nextProps: typeof props) => usePreviewDialogChromeState(nextProps),
		{
			initialProps: props,
			wrapper: ({ children }) => (
				<DialogLifecycle open target={null}>
					{({ complete }) => {
						finishEntrance = complete;
						return children;
					}}
				</DialogLifecycle>
			),
		},
	);
	return { ...hook, dispatch };
}

describe("usePreviewDialogChromeState", () => {
	beforeEach(() => {
		installMatchMedia(false);
		vi.useRealTimers();
	});

	it("uses inner scroll and viewport height for embedded document previews", () => {
		const { result } = renderChrome({ activeOption: pdfOption });

		expect(result.current.usesInnerScroll).toBe(true);
		expect(result.current.fillsViewportHeight).toBe(true);
		expect(result.current.dialogContentClassName.split(/\s+/)).toContain(
			"h-[90vh]",
		);
	});

	it("keeps external url-template viewers out of iframe-style inner scrolling", () => {
		const { result } = renderChrome({ activeOption: externalViewerOption });

		expect(result.current.usesInnerScroll).toBe(false);
		expect(result.current.fillsViewportHeight).toBe(false);
	});

	it("treats embedded url-template viewers like iframe previews", () => {
		const { result } = renderChrome({ activeOption: embeddedViewerOption });

		expect(result.current.usesInnerScroll).toBe(true);
		expect(result.current.fillsViewportHeight).toBe(true);
	});

	it("auto-expands image previews until the user manually chooses a size", () => {
		const { result, rerender } = renderChrome({
			activeOption: imageOption,
			hasManualExpanded: false,
			isExpanded: false,
		});

		expect(result.current.isImagePreview).toBe(true);
		expect(result.current.isExpanded).toBe(true);
		expect(result.current.dialogOverlayClassName).toContain("bg-zinc-950/88");

		rerender({
			activeOption: imageOption,
			dispatch: vi.fn(),
			hasManualExpanded: true,
			isDialogAnimationEnabled: true,
			isExpanded: false,
			open: true,
			showOpenMethodChooser: false,
		});
		expect(result.current.isExpanded).toBe(false);
	});

	it("forces fullscreen layout on mobile preview viewports", () => {
		installMatchMedia(true);
		const { result } = renderChrome({
			activeOption: externalViewerOption,
			isExpanded: false,
		});

		expect(result.current.isExpanded).toBe(true);
		expect(result.current.dialogContentClassName.split(/\s+/)).toContain(
			"max-w-none",
		);
	});

	it("disables open animation after the dialog settles", () => {
		const { dispatch } = renderChrome();
		expect(dispatch).not.toHaveBeenCalled();

		act(() => {
			finishEntrance(true);
		});

		expect(dispatch).toHaveBeenCalledWith({ type: "disableAnimation" });
	});
});
