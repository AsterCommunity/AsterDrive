import { useTranslation } from "react-i18next";
import { Badge } from "@/components/ui/badge";
import { Button } from "@/components/ui/button";
import { Icon } from "@/components/ui/icon";
import { ADMIN_ICON_BUTTON_CLASS } from "@/lib/constants";
import { formatDateTime } from "@/lib/format";
import type {
	RemoteStorageTargetConnectorDescriptor,
	RemoteStorageTargetInfo,
} from "@/types/api";
import {
	getRemoteNodeRemoteStorageTargetConnectorBadgeTone,
	getRemoteNodeRemoteStorageTargetProfileStatus,
} from "./remoteNodeRemoteStorageTargetPresentation";

interface Props {
	connectorDescriptors: RemoteStorageTargetConnectorDescriptor[];
	errorMessage: string | null;
	loading: boolean;
	pendingDeleteTargetKey: string | null;
	readOnly?: boolean;
	onCancelDelete: () => void;
	onConfirmDeleteTarget: (target: RemoteStorageTargetInfo) => void;
	onRequestDeleteTarget: (target: RemoteStorageTargetInfo) => void;
	onEditTarget: (target: RemoteStorageTargetInfo) => void;
	targets: RemoteStorageTargetInfo[];
}
export function RemoteNodeRemoteStorageTargetsList({
	connectorDescriptors,
	errorMessage,
	loading,
	pendingDeleteTargetKey,
	readOnly = false,
	onCancelDelete,
	onConfirmDeleteTarget,
	onRequestDeleteTarget,
	onEditTarget,
	targets,
}: Props) {
	const { t } = useTranslation("admin");
	if (errorMessage) return null;
	if (loading)
		return (
			<div className="mt-4 rounded-2xl border p-4 text-sm text-muted-foreground">
				<Icon name="Spinner" className="mr-2 inline size-4 animate-spin" />
				{t("core:loading")}
			</div>
		);
	if (targets.length === 0)
		return (
			<div className="mt-4 rounded-2xl border border-dashed p-4">
				<p className="text-sm font-medium">
					{t("remote_node_ingress_profiles_empty")}
				</p>
				<p className="mt-1 text-sm text-muted-foreground">
					{t("remote_node_ingress_profiles_empty_desc")}
				</p>
			</div>
		);
	return (
		<div className="mt-4 space-y-3">
			{targets.map((target) => {
				const descriptor = connectorDescriptors.find(
					(item) => item.connector_id === target.connector_id,
				);
				const status = getRemoteNodeRemoteStorageTargetProfileStatus(target);
				const fields =
					descriptor?.fields.filter(
						(field) => field.scope === "connector_config" && !field.secret,
					) ?? [];
				const deleting = pendingDeleteTargetKey === target.target_key;
				return (
					<article
						key={target.target_key}
						className="rounded-2xl border border-border/70 bg-muted/10 p-4"
					>
						<div className="flex items-start justify-between gap-3">
							<div className="flex flex-wrap items-center gap-2">
								<h4 className="text-sm font-semibold">{target.name}</h4>
								<Badge
									variant="outline"
									className={getRemoteNodeRemoteStorageTargetConnectorBadgeTone(
										target.connector_available,
									)}
								>
									{descriptor ? t(descriptor.label_key) : target.connector_id}
								</Badge>
								{target.is_default ? (
									<Badge variant="outline">
										{t("remote_node_ingress_profile_default")}
									</Badge>
								) : null}
								<Badge variant="outline" className={status.toneClass}>
									{t(status.labelKey)}
								</Badge>
							</div>
							{readOnly ? null : (
								<div className="flex gap-1">
									<Button
										type="button"
										variant="ghost"
										size="icon"
										className={ADMIN_ICON_BUTTON_CLASS}
										onClick={() => onEditTarget(target)}
										disabled={!descriptor}
										aria-label={t("core:edit")}
									>
										<Icon
											name="PencilSimple"
											className="size-3.5"
											aria-hidden
										/>
									</Button>
									{deleting ? (
										<>
											<Button
												type="button"
												variant="destructive"
												size="sm"
												onClick={() => onConfirmDeleteTarget(target)}
											>
												{t("core:delete")}
											</Button>
											<Button
												type="button"
												variant="ghost"
												size="sm"
												onClick={onCancelDelete}
											>
												{t("core:cancel")}
											</Button>
										</>
									) : (
										<Button
											type="button"
											variant="ghost"
											size="icon"
											className={`${ADMIN_ICON_BUTTON_CLASS} text-destructive`}
											onClick={() => onRequestDeleteTarget(target)}
											aria-label={t("core:delete")}
										>
											<Icon name="Trash" className="size-3.5" aria-hidden />
										</Button>
									)}
								</div>
							)}
						</div>
						<dl className="mt-4 grid gap-3 text-sm md:grid-cols-2">
							{fields.map((field) => (
								<div key={field.name}>
									<dt className="text-[11px] font-medium uppercase tracking-[0.14em] text-muted-foreground">
										{t(field.label_key)}
									</dt>
									<dd className="mt-1 break-all font-medium">
										{String(
											target.connector_config.values[field.name] ??
												field.default_value ??
												"—",
										)}
									</dd>
								</div>
							))}
							<div>
								<dt className="text-[11px] font-medium uppercase tracking-[0.14em] text-muted-foreground">
									{t("core:updated_at")}
								</dt>
								<dd className="mt-1 font-medium">
									{formatDateTime(target.updated_at)}
								</dd>
							</div>
						</dl>
						{target.last_error || !target.connector_available ? (
							<div className="mt-4 rounded-xl border p-3 text-sm break-all">
								{target.last_error || target.connector_id}
							</div>
						) : null}
					</article>
				);
			})}
		</div>
	);
}
