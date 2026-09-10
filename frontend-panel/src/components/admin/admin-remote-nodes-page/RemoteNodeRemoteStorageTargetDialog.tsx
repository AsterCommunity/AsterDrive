import { useTranslation } from "react-i18next";
import type { RemoteStorageTargetFormData } from "@/components/admin/remoteStorageTargetDialogShared";
import {
	Dialog,
	DialogContent,
	DialogDescription,
	DialogHeader,
	DialogTitle,
} from "@/components/ui/dialog";
import type {
	RemoteStorageTargetInfo,
	StorageConnectorDescriptor,
} from "@/types/api";
import { RemoteNodeRemoteStorageTargetForm } from "./RemoteNodeRemoteStorageTargetForm";
import type {
	RemoteNodeRemoteStorageTargetDraftMode,
	RemoteNodeRemoteStorageTargetFieldChangeHandler,
} from "./RemoteNodeRemoteStorageTargetTypes";

interface RemoteNodeRemoteStorageTargetDialogProps {
	connectorDescriptors: StorageConnectorDescriptor[];
	connectorIdError: string | null;
	draftMode: RemoteNodeRemoteStorageTargetDraftMode;
	editingTarget: RemoteStorageTargetInfo | null;
	form: RemoteStorageTargetFormData;
	nameError: string | null;
	open: boolean;
	onCancel: () => void;
	onFieldChange: RemoteNodeRemoteStorageTargetFieldChangeHandler;
	onOpenChange: (open: boolean) => void;
	onSubmit: () => void;
	submitDisabled: boolean;
	submitting: boolean;
	targets: RemoteStorageTargetInfo[];
}

export function RemoteNodeRemoteStorageTargetDialog({
	connectorDescriptors,
	connectorIdError,
	draftMode,
	editingTarget,
	form,
	nameError,
	open,
	onCancel,
	onFieldChange,
	onOpenChange,
	onSubmit,
	submitDisabled,
	submitting,
	targets,
}: RemoteNodeRemoteStorageTargetDialogProps) {
	const { t } = useTranslation("admin");

	return (
		<Dialog open={open} onOpenChange={onOpenChange}>
			<DialogContent
				className="max-h-[min(90vh,54rem)] overflow-y-auto sm:max-w-[min(46rem,calc(100vw-2rem))]"
				showCloseButton={!submitting}
			>
				<DialogHeader>
					<DialogTitle>
						{draftMode === "create"
							? t("remote_node_ingress_profile_form_create_title")
							: t("remote_node_ingress_profile_form_edit_title")}
					</DialogTitle>
					<DialogDescription>
						{t("remote_node_ingress_profiles_desc")}
					</DialogDescription>
					{editingTarget ? (
						<p className="font-mono text-xs text-muted-foreground">
							{editingTarget.target_key}
						</p>
					) : null}
				</DialogHeader>
				<RemoteNodeRemoteStorageTargetForm
					connectorDescriptors={connectorDescriptors}
					connectorIdError={connectorIdError}
					draftMode={draftMode}
					form={form}
					nameError={nameError}
					onCancel={onCancel}
					onFieldChange={onFieldChange}
					onSubmit={onSubmit}
					presentation="dialog"
					submitDisabled={submitDisabled}
					submitting={submitting}
					targets={targets}
				/>
			</DialogContent>
		</Dialog>
	);
}
