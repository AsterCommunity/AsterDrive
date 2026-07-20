import { create } from "zustand";
import type { Workspace } from "@/lib/workspace";

export const DOWNLOAD_TASK_STATUS = {
	queued: "queued",
	preparing: "preparing",
	downloading: "downloading",
	completed: "completed",
	failed: "failed",
	canceled: "canceled",
} as const;

export type DownloadTaskStatus =
	(typeof DOWNLOAD_TASK_STATUS)[keyof typeof DOWNLOAD_TASK_STATUS];

export interface DownloadSelectionFile {
	id: number;
	name: string;
	size?: number;
}

export interface DownloadSelectionFolder {
	id: number;
	name: string;
}

export interface DownloadSelection {
	workspace: Workspace;
	files: DownloadSelectionFile[];
	folders: DownloadSelectionFolder[];
}

export interface DownloadTaskItem {
	id: string;
	name: string;
	relativePath?: string;
	status: DownloadTaskStatus;
	bytesReceived: number;
	totalBytes: number | null;
	speedBps: number | null;
	error?: string;
}

export interface DownloadTask {
	id: string;
	kind: "file" | "archive" | "directory";
	name: string;
	status: DownloadTaskStatus;
	createdAt: number;
	bytesReceived: number;
	totalBytes: number | null;
	speedBps: number | null;
	completedItems: number;
	failedItems: number;
	totalItems: number;
	items: DownloadTaskItem[];
	error?: string;
	warning?: string;
}

interface DownloadStoreState {
	pendingSelection: DownloadSelection | null;
	tasks: DownloadTask[];
	dismissSelection: () => void;
	removeCompleted: () => void;
	requestSelection: (selection: DownloadSelection) => void;
	upsertTask: (task: DownloadTask) => void;
	updateTask: (id: string, patch: Partial<DownloadTask>) => void;
}

export const useDownloadStore = create<DownloadStoreState>((set) => ({
	pendingSelection: null,
	tasks: [],
	dismissSelection: () => set({ pendingSelection: null }),
	removeCompleted: () =>
		set((state) => ({
			tasks: state.tasks.filter(
				(task) =>
					task.status !== DOWNLOAD_TASK_STATUS.completed &&
					task.status !== DOWNLOAD_TASK_STATUS.canceled,
			),
		})),
	requestSelection: (pendingSelection) => set({ pendingSelection }),
	upsertTask: (task) =>
		set((state) => ({
			tasks: [task, ...state.tasks.filter((item) => item.id !== task.id)],
		})),
	updateTask: (id, patch) =>
		set((state) => ({
			tasks: state.tasks.map((task) =>
				task.id === id ? { ...task, ...patch } : task,
			),
		})),
}));

export function requestDownloadSelection(selection: DownloadSelection) {
	useDownloadStore.getState().requestSelection(selection);
}
