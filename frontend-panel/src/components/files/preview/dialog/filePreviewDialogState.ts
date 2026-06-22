import type { OpenWithMode } from "../capabilities/types";

export type FilePreviewDialogUiState = {
	mode: OpenWithMode | null;
	fileId: number | null;
	isDialogAnimationEnabled: boolean;
	isExpanded: boolean;
	hasManualExpanded: boolean;
	hasConfirmedInitialMode: boolean;
	forceOpenMethodChooser: boolean;
	isDirty: boolean;
	confirmOpen: boolean;
	showAllOpenMethods: boolean;
};
export type FilePreviewDialogUiAction =
	| {
			type: "syncMode";
			fileId: number;
			preferredMode: OpenWithMode | null;
	  }
	| { type: "syncShowAllOpenMethods"; showAllOpenMethods: boolean }
	| { type: "selectOpenMethod"; mode: OpenWithMode }
	| { type: "openMethodPickerOpened" }
	| { type: "setConfirmOpen"; confirmOpen: boolean }
	| { type: "setDirty"; isDirty: boolean }
	| { type: "discardChanges" }
	| { type: "setExpanded"; expanded: boolean }
	| { type: "disableAnimation" }
	| { type: "showAllOpenMethods" };

export const initialFilePreviewDialogUiState: FilePreviewDialogUiState = {
	mode: null,
	fileId: null,
	isDialogAnimationEnabled: true,
	isExpanded: false,
	hasManualExpanded: false,
	hasConfirmedInitialMode: false,
	forceOpenMethodChooser: false,
	isDirty: false,
	confirmOpen: false,
	showAllOpenMethods: false,
};

export function filePreviewDialogUiReducer(
	state: FilePreviewDialogUiState,
	action: FilePreviewDialogUiAction,
): FilePreviewDialogUiState {
	switch (action.type) {
		case "syncMode": {
			const resetForFile = state.fileId !== action.fileId;
			return {
				...state,
				fileId: action.fileId,
				mode: action.preferredMode,
				hasConfirmedInitialMode: resetForFile
					? false
					: state.hasConfirmedInitialMode,
				hasManualExpanded: resetForFile ? false : state.hasManualExpanded,
				isExpanded: resetForFile ? false : state.isExpanded,
				isDirty: resetForFile ? false : state.isDirty,
				confirmOpen: resetForFile ? false : state.confirmOpen,
				forceOpenMethodChooser: resetForFile
					? false
					: state.forceOpenMethodChooser,
			};
		}
		case "syncShowAllOpenMethods":
			if (state.showAllOpenMethods === action.showAllOpenMethods) {
				return state;
			}
			return {
				...state,
				showAllOpenMethods: action.showAllOpenMethods,
			};
		case "selectOpenMethod":
			return {
				...state,
				mode: action.mode,
				isDialogAnimationEnabled: true,
				forceOpenMethodChooser: false,
				hasConfirmedInitialMode: true,
			};
		case "openMethodPickerOpened":
			return {
				...state,
				isDialogAnimationEnabled: true,
				forceOpenMethodChooser: true,
				hasConfirmedInitialMode: false,
				showAllOpenMethods: false,
			};
		case "setConfirmOpen":
			if (state.confirmOpen === action.confirmOpen) {
				return state;
			}
			return {
				...state,
				confirmOpen: action.confirmOpen,
			};
		case "setDirty":
			if (state.isDirty === action.isDirty) {
				return state;
			}
			return {
				...state,
				isDirty: action.isDirty,
			};
		case "discardChanges":
			return {
				...state,
				confirmOpen: false,
				isDirty: false,
			};
		case "setExpanded":
			if (state.isExpanded === action.expanded && state.hasManualExpanded) {
				return state;
			}
			return {
				...state,
				hasManualExpanded: true,
				isDialogAnimationEnabled: false,
				isExpanded: action.expanded,
			};
		case "disableAnimation":
			if (!state.isDialogAnimationEnabled) {
				return state;
			}
			return {
				...state,
				isDialogAnimationEnabled: false,
			};
		case "showAllOpenMethods":
			if (state.showAllOpenMethods) {
				return state;
			}
			return {
				...state,
				showAllOpenMethods: true,
			};
	}
}
