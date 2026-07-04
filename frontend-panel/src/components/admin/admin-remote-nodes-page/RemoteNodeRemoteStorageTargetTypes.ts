import type { RemoteStorageTargetFormData } from "@/components/admin/remoteStorageTargetDialogShared";

export type RemoteNodeRemoteStorageTargetDraftMode = "create" | "edit";

export type RemoteNodeRemoteStorageTargetFieldChangeHandler = <
	K extends keyof RemoteStorageTargetFormData,
>(
	key: K,
	value: RemoteStorageTargetFormData[K],
) => void;
