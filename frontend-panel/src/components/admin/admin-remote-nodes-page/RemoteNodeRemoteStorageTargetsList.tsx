import { useMemo } from "react";
import { useTranslation } from "react-i18next";
import {
	ADMIN_INTERACTIVE_TABLE_ROW_CLASS,
	ADMIN_TABLE_BADGE_CELL_CLASS,
	ADMIN_TABLE_MONO_TEXT_CLASS,
	ADMIN_TABLE_TEXT_CELL_CLASS,
	AdminTableCell as TableCell,
	AdminTableHead as TableHead,
	AdminTableHeader as TableHeader,
	AdminTableRow as TableRow,
} from "@/components/common/AdminTable";
import { AdminTableList } from "@/components/common/AdminTableList";
import { Badge } from "@/components/ui/badge";
import { Button } from "@/components/ui/button";
import { Icon } from "@/components/ui/icon";
import { translateStorageConnectorMessage } from "@/lib/adminStorageConnectorLocalizations";
import { ADMIN_ICON_BUTTON_CLASS } from "@/lib/constants";
import { formatDateTime } from "@/lib/format";
import type {
	RemoteStorageTargetInfo,
	StorageConnectorDescriptor,
} from "@/types/api";
import { getStorageConnectorBadgePresentation } from "../admin-policies-page/policyPresentation";
import { getRemoteNodeRemoteStorageTargetProfileStatus } from "./remoteNodeRemoteStorageTargetPresentation";

interface RemoteNodeRemoteStorageTargetsListProps {
	errorMessage: string | null;
	loading: boolean;
	pendingDeleteTargetKey: string | null;
	readOnly?: boolean;
	onCancelDelete: () => void;
	onConfirmDeleteTarget: (
		target: RemoteStorageTargetInfo,
	) => void | Promise<void>;
	onRequestDeleteTarget: (target: RemoteStorageTargetInfo) => void;
	onEditTarget: (target: RemoteStorageTargetInfo) => void;
	targets: RemoteStorageTargetInfo[];
	connectorDescriptors: StorageConnectorDescriptor[];
}

export function RemoteNodeRemoteStorageTargetsList({
	errorMessage,
	loading,
	pendingDeleteTargetKey,
	readOnly = false,
	onCancelDelete,
	onConfirmDeleteTarget,
	onRequestDeleteTarget,
	onEditTarget,
	targets,
	connectorDescriptors,
}: RemoteNodeRemoteStorageTargetsListProps) {
	const { t } = useTranslation("admin");
	const descriptorByConnectorId = useMemo(
		() =>
			new Map(
				connectorDescriptors.map((descriptor) => [
					descriptor.connector_id,
					descriptor,
				]),
			),
		[connectorDescriptors],
	);
	const headerRow = (
		<TableHeader>
			<TableRow>
				<TableHead>{t("core:name")}</TableHead>
				<TableHead>{t("remote_storage_target")}</TableHead>
				<TableHead>{t("connector_type")}</TableHead>
				<TableHead>{t("remote_node_status")}</TableHead>
				<TableHead>{t("core:updated_at")}</TableHead>
				{readOnly ? null : <TableHead>{t("core:actions")}</TableHead>}
			</TableRow>
		</TableHeader>
	);
	if (errorMessage) return null;

	return (
		<AdminTableList
			className="mt-4"
			columns={readOnly ? 5 : 6}
			emptyDescription={t("remote_node_ingress_profiles_empty_desc")}
			emptyTitle={t("remote_node_ingress_profiles_empty")}
			frameless
			headerRow={headerRow}
			items={targets}
			loading={loading}
			renderRow={(target) => {
				const descriptor = descriptorByConnectorId.get(
					target.connector_id ?? "",
				);
				const status = getRemoteNodeRemoteStorageTargetProfileStatus(target);
				const badgePresentation = getStorageConnectorBadgePresentation(
					descriptor?.ui.badge_rgb,
				);
				const connectorT = (key: string) =>
					translateStorageConnectorMessage(t, descriptor?.connector_id, key);
				const connectorLabel = descriptor
					? connectorT(descriptor.ui.label_key)
					: (target.connector_id ?? "unknown");
				const configurationSummary = buildConfigurationSummary(
					descriptor,
					target,
					connectorT,
				);
				const deleteConfirming = pendingDeleteTargetKey === target.target_key;

				return (
					<TableRow
						key={target.target_key}
						className={ADMIN_INTERACTIVE_TABLE_ROW_CLASS}
						onClick={() => {
							if (!readOnly && !deleteConfirming) onEditTarget(target);
						}}
						onKeyDown={(event) => {
							if (
								!readOnly &&
								!deleteConfirming &&
								(event.key === "Enter" || event.key === " ")
							) {
								event.preventDefault();
								onEditTarget(target);
							}
						}}
						tabIndex={readOnly ? undefined : 0}
					>
						<TableCell>
							<div className="min-w-0">
								<div className="truncate font-medium text-foreground">
									{target.name}
								</div>
								<div className={ADMIN_TABLE_MONO_TEXT_CLASS}>
									{target.target_key}
								</div>
							</div>
						</TableCell>
						<TableCell>
							<div className={ADMIN_TABLE_TEXT_CELL_CLASS}>
								<span className="line-clamp-2 text-xs text-muted-foreground">
									{configurationSummary}
								</span>
							</div>
						</TableCell>
						<TableCell>
							<div className={ADMIN_TABLE_BADGE_CELL_CLASS}>
								<Badge
									variant="outline"
									className={badgePresentation.className}
									style={badgePresentation.style}
								>
									{connectorLabel}
								</Badge>
							</div>
						</TableCell>
						<TableCell>
							<div className={ADMIN_TABLE_BADGE_CELL_CLASS}>
								<Badge variant="outline" className={status.toneClass}>
									{t(status.labelKey)}
								</Badge>
							</div>
						</TableCell>
						<TableCell>
							<span className="text-xs text-muted-foreground">
								{formatDateTime(target.updated_at)}
							</span>
						</TableCell>
						{readOnly ? null : (
							<TableCell
								onClick={(event) => event.stopPropagation()}
								onKeyDown={(event) => event.stopPropagation()}
							>
								{deleteConfirming ? (
									<div className="flex flex-wrap items-center justify-end gap-2 duration-150 animate-in fade-in slide-in-from-top-1 motion-reduce:animate-none">
										<div className="mr-auto min-w-40 text-left duration-150 animate-in fade-in slide-in-from-top-1 motion-reduce:animate-none">
											<p className="text-xs font-medium text-destructive">
												{t("remote_node_ingress_profile_delete_title", {
													name: target.name,
												})}
											</p>
											<p className="text-xs text-muted-foreground">
												{t("remote_node_ingress_profile_delete_desc")}
											</p>
										</div>
										<Button
											type="button"
											variant="destructive"
											size="sm"
											onClick={() => void onConfirmDeleteTarget(target)}
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
									</div>
								) : (
									<div className="flex justify-end gap-1">
										<Button
											type="button"
											variant="ghost"
											size="icon"
											className={ADMIN_ICON_BUTTON_CLASS}
											onClick={() => onEditTarget(target)}
											aria-label={t("core:edit")}
											title={t("core:edit")}
										>
											<Icon name="PencilSimple" className="size-3.5" />
										</Button>
										<Button
											type="button"
											variant="ghost"
											size="icon"
											className={`${ADMIN_ICON_BUTTON_CLASS} text-destructive`}
											onClick={() => onRequestDeleteTarget(target)}
											aria-label={t("core:delete")}
											title={t("core:delete")}
										>
											<Icon name="Trash" className="size-3.5" />
										</Button>
									</div>
								)}
							</TableCell>
						)}
					</TableRow>
				);
			}}
		/>
	);
}

function buildConfigurationSummary(
	descriptor: StorageConnectorDescriptor | undefined,
	target: RemoteStorageTargetInfo,
	connectorT: (key: string) => string,
) {
	const values = target.connector_config?.values ?? {};
	const parts = (descriptor?.fields ?? [])
		.filter(
			(field) =>
				field.scope === "connector_config" &&
				!field.secret &&
				values[field.name] !== undefined &&
				values[field.name] !== null &&
				values[field.name] !== "",
		)
		.slice(0, 2)
		.map(
			(field) =>
				`${connectorT(field.label_key)}: ${String(values[field.name])}`,
		);
	return parts.length > 0 ? parts.join(" · ") : "-";
}
