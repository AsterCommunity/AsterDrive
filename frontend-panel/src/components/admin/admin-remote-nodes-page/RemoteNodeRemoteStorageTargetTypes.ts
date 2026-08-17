import type {
	RemoteStorageTargetFieldValue,
	RemoteStorageTargetFormData,
} from "@/components/admin/remoteStorageTargetDialogShared";

export type RemoteNodeRemoteStorageTargetDraftMode = "create" | "edit";
export type RemoteNodeRemoteStorageTargetFieldChangeHandler = (
	key: "name" | "connector_id" | "is_default" | "value",
	value:
		| string
		| boolean
		| { name: string; value: RemoteStorageTargetFieldValue },
) => void;
export type { RemoteStorageTargetFormData };
