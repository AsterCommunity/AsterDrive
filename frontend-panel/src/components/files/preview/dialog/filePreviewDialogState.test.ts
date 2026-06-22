import { describe, expect, it } from "vitest";
import {
	filePreviewDialogUiReducer,
	initialFilePreviewDialogUiState,
} from "./filePreviewDialogState";

describe("filePreviewDialogUiReducer", () => {
	it("resets file-scoped UI state when the file changes", () => {
		const selected = filePreviewDialogUiReducer(
			initialFilePreviewDialogUiState,
			{ type: "selectOpenMethod", mode: "builtin.markdown" },
		);
		const dirty = filePreviewDialogUiReducer(selected, {
			type: "setDirty",
			isDirty: true,
		});
		const confirming = filePreviewDialogUiReducer(dirty, {
			type: "setConfirmOpen",
			confirmOpen: true,
		});
		const expanded = filePreviewDialogUiReducer(confirming, {
			type: "setExpanded",
			expanded: true,
		});
		const reset = filePreviewDialogUiReducer(expanded, {
			type: "syncMode",
			fileId: 7,
			preferredMode: "builtin.code",
		});

		expect(reset).toMatchObject({
			forceOpenMethodChooser: false,
			confirmOpen: false,
			hasConfirmedInitialMode: false,
			hasManualExpanded: false,
			isDirty: false,
			isExpanded: false,
			mode: "builtin.code",
		});
	});

	it("keeps file-scoped UI state when only preferred mode refreshes", () => {
		const confirmed = filePreviewDialogUiReducer(
			initialFilePreviewDialogUiState,
			{ type: "selectOpenMethod", mode: "builtin.markdown" },
		);
		const synced = filePreviewDialogUiReducer(confirmed, {
			type: "syncMode",
			fileId: null,
			preferredMode: "builtin.code",
		});

		expect(synced).toMatchObject({
			hasConfirmedInitialMode: true,
			mode: "builtin.code",
		});
	});

	it("opens the method picker and clears expanded hidden methods", () => {
		const showingAll = filePreviewDialogUiReducer(
			initialFilePreviewDialogUiState,
			{ type: "showAllOpenMethods" },
		);
		const picker = filePreviewDialogUiReducer(showingAll, {
			type: "openMethodPickerOpened",
		});

		expect(picker).toMatchObject({
			forceOpenMethodChooser: true,
			hasConfirmedInitialMode: false,
			isDialogAnimationEnabled: true,
			showAllOpenMethods: false,
		});
	});

	it("tracks dirty confirmation and discard state together", () => {
		const dirty = filePreviewDialogUiReducer(initialFilePreviewDialogUiState, {
			type: "setDirty",
			isDirty: true,
		});
		const confirming = filePreviewDialogUiReducer(dirty, {
			type: "setConfirmOpen",
			confirmOpen: true,
		});
		const discarded = filePreviewDialogUiReducer(confirming, {
			type: "discardChanges",
		});

		expect(discarded).toMatchObject({
			confirmOpen: false,
			isDirty: false,
		});
	});

	it("marks expansion as manual and ignores identical repeated expansion actions", () => {
		const expanded = filePreviewDialogUiReducer(
			initialFilePreviewDialogUiState,
			{
				type: "setExpanded",
				expanded: true,
			},
		);
		const repeated = filePreviewDialogUiReducer(expanded, {
			type: "setExpanded",
			expanded: true,
		});

		expect(expanded).toMatchObject({
			hasManualExpanded: true,
			isDialogAnimationEnabled: false,
			isExpanded: true,
		});
		expect(repeated).toBe(expanded);
	});
});
