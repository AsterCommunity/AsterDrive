import type { FileStoreSlice, SelectionSlice } from "./types";

export const createSelectionSlice: FileStoreSlice<SelectionSlice> = (
	set,
	get,
) => ({
	selectedFileIds: new Set(),
	selectedFolderIds: new Set(),

	toggleFileSelection: (id) => {
		const next = new Set(get().selectedFileIds);
		if (next.has(id)) {
			next.delete(id);
		} else {
			next.add(id);
		}
		set({ selectedFileIds: next });
	},

	toggleFolderSelection: (id) => {
		const next = new Set(get().selectedFolderIds);
		if (next.has(id)) {
			next.delete(id);
		} else {
			next.add(id);
		}
		set({ selectedFolderIds: next });
	},

	selectOnlyFile: (id) => {
		set({
			selectedFileIds: new Set([id]),
			selectedFolderIds: new Set(),
		});
	},

	selectOnlyFolder: (id) => {
		set({
			selectedFileIds: new Set(),
			selectedFolderIds: new Set([id]),
		});
	},

	selectItems: (fileIds, folderIds) => {
		set({
			selectedFileIds: new Set(fileIds),
			selectedFolderIds: new Set(folderIds),
		});
	},

	selectAll: () => {
		const { files, folders } = get();
		get().selectItems(
			files.map((file) => file.id),
			folders.map((folder) => folder.id),
		);
	},

	clearSelection: () => {
		set({
			selectedFileIds: new Set(),
			selectedFolderIds: new Set(),
		});
	},

	selectionCount: () => {
		const { selectedFileIds, selectedFolderIds } = get();
		return selectedFileIds.size + selectedFolderIds.size;
	},
});
