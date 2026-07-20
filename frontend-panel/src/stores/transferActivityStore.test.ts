import { beforeEach, describe, expect, it } from "vitest";
import {
	TRANSFER_ACTIVITY,
	useTransferActivityStore,
} from "@/stores/transferActivityStore";

describe("transferActivityStore", () => {
	beforeEach(() => {
		useTransferActivityStore.setState({ expandedActivity: null });
	});

	it("keeps upload and download details mutually exclusive", () => {
		const { setActivityOpen } = useTransferActivityStore.getState();

		setActivityOpen(TRANSFER_ACTIVITY.upload, true);
		expect(useTransferActivityStore.getState().expandedActivity).toBe(
			TRANSFER_ACTIVITY.upload,
		);

		setActivityOpen(TRANSFER_ACTIVITY.download, true);
		expect(useTransferActivityStore.getState().expandedActivity).toBe(
			TRANSFER_ACTIVITY.download,
		);
	});

	it("supports toggle updaters without closing another activity", () => {
		const { setActivityOpen } = useTransferActivityStore.getState();
		setActivityOpen(TRANSFER_ACTIVITY.download, true);

		setActivityOpen(TRANSFER_ACTIVITY.upload, false);
		expect(useTransferActivityStore.getState().expandedActivity).toBe(
			TRANSFER_ACTIVITY.download,
		);

		setActivityOpen(TRANSFER_ACTIVITY.download, (open) => !open);
		expect(useTransferActivityStore.getState().expandedActivity).toBeNull();
	});
});
