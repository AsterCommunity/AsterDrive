import { create } from "zustand";

export const TRANSFER_ACTIVITY = {
	upload: "upload",
	download: "download",
} as const;

export type TransferActivity =
	(typeof TRANSFER_ACTIVITY)[keyof typeof TRANSFER_ACTIVITY];

type ActivityOpenUpdater = boolean | ((open: boolean) => boolean);

interface TransferActivityStoreState {
	expandedActivity: TransferActivity | null;
	setActivityOpen: (
		activity: TransferActivity,
		open: ActivityOpenUpdater,
	) => void;
}

export const useTransferActivityStore = create<TransferActivityStoreState>(
	(set) => ({
		expandedActivity: null,
		setActivityOpen: (activity, nextOpen) =>
			set((state) => {
				const currentlyOpen = state.expandedActivity === activity;
				const open =
					typeof nextOpen === "function" ? nextOpen(currentlyOpen) : nextOpen;
				if (open) return { expandedActivity: activity };
				return currentlyOpen ? { expandedActivity: null } : state;
			}),
	}),
);
