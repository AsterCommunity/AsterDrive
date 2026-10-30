import { useEffect, useReducer } from "react";
import { useDialogPresentation } from "@/components/ui/dialog-lifecycle";
import { ApiPendingError, isRequestCanceled } from "@/services/http";
import type {
	ArchivePreviewAction,
	ArchivePreviewLoadManifest,
	ArchivePreviewState,
} from "./archivePreviewTypes";
import { classifyArchivePreviewError } from "./archivePreviewUtils";

function createArchivePreviewState(
	loadManifest: ArchivePreviewLoadManifest | undefined,
): ArchivePreviewState {
	return {
		manifest: null,
		query: "",
		currentFolder: null,
		loading: Boolean(loadManifest),
		pending: false,
		error: null,
		reloadKey: 0,
		filenameEncoding: "auto",
	};
}

function archivePreviewReducer(
	state: ArchivePreviewState,
	action: ArchivePreviewAction,
): ArchivePreviewState {
	switch (action.type) {
		case "manifestLoaderUnavailable":
			return {
				...state,
				manifest: null,
				loading: false,
				pending: false,
				error: null,
			};
		case "loadStarted":
			return {
				...state,
				loading: true,
				error: null,
			};
		case "loadSucceeded":
			return {
				...state,
				manifest: action.manifest,
				currentFolder: null,
				loading: false,
				pending: false,
				error: null,
			};
		case "loadPending":
			return {
				...state,
				loading: true,
				pending: true,
			};
		case "loadFailed":
			return {
				...state,
				loading: false,
				pending: false,
				error: action.error,
			};
		case "retryRequested":
			return {
				...state,
				loading: true,
				pending: state.pending,
				error: null,
				reloadKey: state.reloadKey + 1,
			};
		case "queryChanged":
			return {
				...state,
				query: action.query,
			};
		case "currentFolderChanged":
			return {
				...state,
				currentFolder: action.currentFolder,
			};
		case "directoryOpened":
			return {
				...state,
				currentFolder: action.path,
				query: "",
			};
		case "filenameEncodingChanged":
			if (state.filenameEncoding === action.filenameEncoding) {
				return state;
			}
			return {
				...state,
				manifest: null,
				query: "",
				currentFolder: null,
				loading: true,
				pending: true,
				error: null,
				reloadKey: state.reloadKey + 1,
				filenameEncoding: action.filenameEncoding,
			};
	}
}

export function useArchivePreviewState(
	loadManifest: ArchivePreviewLoadManifest | undefined,
) {
	const active = useDialogPresentation()?.active ?? true;
	const [state, dispatch] = useReducer(
		archivePreviewReducer,
		loadManifest,
		createArchivePreviewState,
	);
	const { filenameEncoding, reloadKey } = state;

	useEffect(() => {
		if (!active) return;
		void reloadKey;
		if (!loadManifest) {
			dispatch({ type: "manifestLoaderUnavailable" });
			return;
		}

		let cancelled = false;
		let retryTimer: number | undefined;
		const controller = new AbortController();
		dispatch({ type: "loadStarted" });
		loadManifest({ signal: controller.signal, filenameEncoding })
			.then((nextManifest) => {
				if (!cancelled) {
					dispatch({ type: "loadSucceeded", manifest: nextManifest });
				}
			})
			.catch((error: unknown) => {
				if (!cancelled && !isRequestCanceled(error)) {
					if (error instanceof ApiPendingError) {
						dispatch({ type: "loadPending" });
						retryTimer = window.setTimeout(
							() => dispatch({ type: "retryRequested" }),
							Math.max(1, error.retryAfterSeconds) * 1000,
						);
						return;
					}
					dispatch({
						type: "loadFailed",
						error: classifyArchivePreviewError(error),
					});
				}
			});

		return () => {
			cancelled = true;
			if (retryTimer !== undefined) {
				window.clearTimeout(retryTimer);
			}
			controller.abort();
		};
	}, [active, filenameEncoding, loadManifest, reloadKey]);

	return [state, dispatch] as const;
}
