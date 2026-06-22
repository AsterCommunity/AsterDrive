import { useCallback, useEffect, useMemo } from "react";
import { usePreviewAppStore } from "@/stores/previewAppStore";
import { useThumbnailSupportStore } from "@/stores/thumbnailSupportStore";
import type { FileInfo, FileListItem } from "@/types/api";
import { detectFilePreviewProfile } from "../capabilities/file-capabilities";
import type { OpenWithOption } from "../capabilities/types";
import { getVideoBrowserOpenWithOption } from "../capabilities/video-browser-config";

interface UsePreviewCapabilitiesOptions {
	archiveManifestAvailable: boolean;
	file: FileInfo | FileListItem;
	wopiSessionAvailable: boolean;
}

export function usePreviewCapabilities({
	archiveManifestAvailable,
	file,
	wopiSessionAvailable,
}: UsePreviewCapabilitiesOptions) {
	const previewApps = usePreviewAppStore((state) => state.config);
	const previewAppsLoaded = usePreviewAppStore((state) => state.isLoaded);
	const loadPreviewApps = usePreviewAppStore((state) => state.load);
	const thumbnailSupport = useThumbnailSupportStore((state) => state.config);
	const thumbnailSupportLoaded = useThumbnailSupportStore(
		(state) => state.isLoaded,
	);
	const loadThumbnailSupport = useThumbnailSupportStore((state) => state.load);

	useEffect(() => {
		if (previewAppsLoaded) return;
		void loadPreviewApps();
	}, [loadPreviewApps, previewAppsLoaded]);

	useEffect(() => {
		if (thumbnailSupportLoaded) return;
		void loadThumbnailSupport();
	}, [loadThumbnailSupport, thumbnailSupportLoaded]);

	const baseProfile = useMemo(() => {
		if (!previewAppsLoaded) return null;
		return detectFilePreviewProfile(file, previewApps, thumbnailSupport);
	}, [file, previewApps, previewAppsLoaded, thumbnailSupport]);

	const customVideoBrowserOption = useMemo(
		() => getVideoBrowserOpenWithOption(),
		[],
	);

	const profile = useMemo(() => {
		if (!baseProfile) return null;
		if (
			baseProfile.category !== "video" ||
			!customVideoBrowserOption ||
			baseProfile.options.some(
				(option) => option.key === customVideoBrowserOption.key,
			)
		) {
			return baseProfile;
		}

		return {
			...baseProfile,
			allOptions: [
				...(baseProfile.allOptions ?? baseProfile.options),
				customVideoBrowserOption,
			],
			options: [...baseProfile.options, customVideoBrowserOption],
		};
	}, [baseProfile, customVideoBrowserOption]);

	const isOptionAvailable = useCallback(
		(option: OpenWithOption) =>
			(option.mode !== "wopi" || wopiSessionAvailable) &&
			(option.mode !== "archive" || archiveManifestAvailable),
		[archiveManifestAvailable, wopiSessionAvailable],
	);

	const allOptions = useMemo(
		() =>
			(profile?.allOptions ?? profile?.options ?? []).filter(isOptionAvailable),
		[isOptionAvailable, profile],
	);
	const visibleOptions = useMemo(() => {
		if (!profile || profile.options.length === 0) {
			return allOptions;
		}

		const nextVisibleOptions = profile.options.filter(isOptionAvailable);
		return nextVisibleOptions.length > 0 ? nextVisibleOptions : allOptions;
	}, [allOptions, isOptionAvailable, profile]);
	const hiddenOptions = useMemo(
		() =>
			allOptions.filter(
				(option) =>
					!visibleOptions.some((candidate) => candidate.key === option.key),
			),
		[allOptions, visibleOptions],
	);

	const preferredMode = useMemo(() => {
		if (!profile) return null;
		if (
			profile.defaultMode &&
			allOptions.some((option) => option.key === profile.defaultMode)
		) {
			return profile.defaultMode;
		}
		return allOptions[0]?.key ?? null;
	}, [allOptions, profile]);

	return {
		allOptions,
		hiddenOptions,
		preferredMode,
		previewAppsLoaded,
		profile,
		visibleOptions,
	};
}
