import type { FileStoreSlice, RequestSlice } from "./types";
import { createWorkspaceResetState } from "./types";

export const createRequestSlice: FileStoreSlice<RequestSlice> = (set, get) => ({
	resetWorkspaceState: () => {
		get()._workspaceRequestController?.abort();
		set((state) => ({
			workspaceRequestRevision: state.workspaceRequestRevision + 1,
			lastFolderContents: null,
			_workspaceRequestId: 0,
			_workspaceRequestController: null,
			...createWorkspaceResetState(),
		}));
	},
	lastFolderContents: null,
	workspaceRequestRevision: 0,
	_workspaceRequestId: 0,
	_workspaceRequestController: null,
});
