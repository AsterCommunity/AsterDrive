import type { DragEvent } from "react";
import { create } from "zustand";

export interface UploadAreaControls {
	isDragging: boolean;
	handleDragEnter: (event: DragEvent<HTMLDivElement>) => void;
	handleDragLeave: (event: DragEvent<HTMLDivElement>) => void;
	handleDragOver: (event: DragEvent<HTMLDivElement>) => void;
	handleDrop: (event: DragEvent<HTMLDivElement>) => Promise<void>;
	triggerFileUpload: () => void;
	triggerFolderUpload: () => void;
}

export interface UploadPanelPresence {
	visible: boolean;
	open: boolean;
}

interface UploadAreaControlsState {
	controls: UploadAreaControls | null;
	uploadPanelPresence: UploadPanelPresence;
	setControls: (controls: UploadAreaControls | null) => void;
	setUploadPanelPresence: (presence: UploadPanelPresence) => void;
}

export const useUploadAreaControlsStore = create<UploadAreaControlsState>(
	(set) => ({
		controls: null,
		uploadPanelPresence: {
			open: false,
			visible: false,
		},
		setControls: (controls) => set({ controls }),
		setUploadPanelPresence: (uploadPanelPresence) =>
			set({ uploadPanelPresence }),
	}),
);
