import type { RemoteNodeFormData } from "../remoteNodePageShared";

export interface RemoteNodeSummaryItem {
	label: string;
	value: string;
}

export type RemoteNodeFieldChangeHandler = <K extends keyof RemoteNodeFormData>(
	key: K,
	value: RemoteNodeFormData[K],
) => void;
