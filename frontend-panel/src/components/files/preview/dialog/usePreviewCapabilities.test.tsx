import { renderHook } from "@testing-library/react";
import { beforeEach, describe, expect, it, vi } from "vitest";
import type { FileListItem } from "@/types/api";
import type { FilePreviewProfile, OpenWithOption } from "../capabilities/types";
import { usePreviewCapabilities } from "./usePreviewCapabilities";

const codeOption: OpenWithOption = {
	icon: "TextT",
	key: "builtin.code",
	labelKey: "open_with_code",
	mode: "code",
};
const archiveOption: OpenWithOption = {
	icon: "Archive",
	key: "builtin.archive",
	labelKey: "open_with_archive",
	mode: "archive",
};
const wopiOption: OpenWithOption = {
	icon: "FileText",
	key: "onlyoffice",
	labelKey: "open_with_onlyoffice",
	mode: "wopi",
};
const videoOption: OpenWithOption = {
	icon: "Video",
	key: "builtin.video",
	labelKey: "open_with_video",
	mode: "video",
};

const mockState = vi.hoisted(() => ({
	detectFilePreviewProfile: vi.fn(),
	previewAppStore: {
		config: null,
		isLoaded: true,
		load: vi.fn(),
	},
	thumbnailSupportStore: {
		config: null,
		isLoaded: true,
		load: vi.fn(),
	},
	videoBrowserOption: null as OpenWithOption | null,
}));

vi.mock("@/stores/previewAppStore", () => ({
	usePreviewAppStore: (
		selector: (state: typeof mockState.previewAppStore) => unknown,
	) => selector(mockState.previewAppStore),
}));

vi.mock("@/stores/thumbnailSupportStore", () => ({
	useThumbnailSupportStore: (
		selector: (state: typeof mockState.thumbnailSupportStore) => unknown,
	) => selector(mockState.thumbnailSupportStore),
}));

vi.mock("../capabilities/file-capabilities", () => ({
	detectFilePreviewProfile: (...args: unknown[]) =>
		mockState.detectFilePreviewProfile(...args),
}));

vi.mock("../capabilities/video-browser-config", () => ({
	getVideoBrowserOpenWithOption: () => mockState.videoBrowserOption,
}));

function file(overrides: Partial<FileListItem> = {}): FileListItem {
	return {
		compound_extension: null,
		extension: "md",
		file_category: "document",
		id: 7,
		lock_state: { state: "unlocked" },
		is_shared: false,
		mime_type: "text/markdown",
		name: "notes.md",
		size: 128,
		updated_at: "2026-01-01T00:00:00Z",
		...overrides,
	};
}

function profile(
	overrides: Partial<FilePreviewProfile> = {},
): FilePreviewProfile {
	return {
		category: "markdown",
		defaultMode: "builtin.code",
		isBlobPreview: false,
		isEditableText: true,
		isTextBased: true,
		options: [codeOption],
		...overrides,
	};
}

function renderCapabilities(
	overrides: Partial<Parameters<typeof usePreviewCapabilities>[0]> = {},
) {
	return renderHook(() =>
		usePreviewCapabilities({
			archiveManifestAvailable: false,
			file: file(),
			wopiSessionAvailable: false,
			...overrides,
		}),
	);
}

describe("usePreviewCapabilities", () => {
	beforeEach(() => {
		mockState.detectFilePreviewProfile.mockReset();
		mockState.detectFilePreviewProfile.mockReturnValue(profile());
		mockState.previewAppStore.config = null;
		mockState.previewAppStore.isLoaded = true;
		mockState.previewAppStore.load.mockReset();
		mockState.thumbnailSupportStore.config = null;
		mockState.thumbnailSupportStore.isLoaded = true;
		mockState.thumbnailSupportStore.load.mockReset();
		mockState.videoBrowserOption = null;
	});

	it("loads capability stores before exposing a profile", () => {
		mockState.previewAppStore.isLoaded = false;
		mockState.thumbnailSupportStore.isLoaded = false;

		const { result } = renderCapabilities();

		expect(mockState.previewAppStore.load).toHaveBeenCalledTimes(1);
		expect(mockState.thumbnailSupportStore.load).toHaveBeenCalledTimes(1);
		expect(result.current.profile).toBeNull();
		expect(result.current.previewAppsLoaded).toBe(false);
	});

	it("filters archive and WOPI options until their backing actions are available", () => {
		mockState.detectFilePreviewProfile.mockReturnValue(
			profile({
				allOptions: [codeOption, archiveOption, wopiOption],
				defaultMode: "builtin.archive",
				options: [archiveOption, wopiOption],
			}),
		);

		const { result, rerender } = renderCapabilities();

		expect(result.current.visibleOptions.map((option) => option.key)).toEqual([
			"builtin.code",
		]);
		expect(result.current.preferredMode).toBe("builtin.code");

		rerender();
		const available = renderCapabilities({
			archiveManifestAvailable: true,
			wopiSessionAvailable: true,
		});
		expect(
			available.result.current.visibleOptions.map((option) => option.key),
		).toEqual(["builtin.archive", "onlyoffice"]);
		expect(available.result.current.preferredMode).toBe("builtin.archive");
	});

	it("adds a configured video browser option without duplicating existing options", () => {
		mockState.videoBrowserOption = {
			icon: "Globe",
			key: "external.video-browser",
			labelKey: "open_with_video_browser",
			mode: "url_template",
		};
		mockState.detectFilePreviewProfile.mockReturnValue(
			profile({
				category: "video",
				defaultMode: "builtin.video",
				isBlobPreview: true,
				isEditableText: false,
				isTextBased: false,
				options: [videoOption],
			}),
		);

		const { result } = renderCapabilities();

		expect(result.current.allOptions.map((option) => option.key)).toEqual([
			"builtin.video",
			"external.video-browser",
		]);
	});
});
