import { useTranslation } from "react-i18next";
import { Badge } from "@/components/ui/badge";
import { Button } from "@/components/ui/button";
import { Icon } from "@/components/ui/icon";
import { ADMIN_ICON_BUTTON_CLASS } from "@/lib/constants";
import { formatBytes, formatDateTime } from "@/lib/format";
import type { RemoteStorageTargetInfo } from "@/types/api";
import {
	getRemoteNodeRemoteStorageTargetDriverBadgeTone,
	getRemoteNodeRemoteStorageTargetProfileStatus,
} from "./remoteNodeRemoteStorageTargetPresentation";

interface RemoteNodeRemoteStorageTargetsListProps {
	errorMessage: string | null;
	loading: boolean;
	pendingDeleteTargetKey: string | null;
	onCancelDelete: () => void;
	onConfirmDeleteTarget: (target: RemoteStorageTargetInfo) => void;
	onRequestDeleteTarget: (target: RemoteStorageTargetInfo) => void;
	onEditTarget: (target: RemoteStorageTargetInfo) => void;
	targets: RemoteStorageTargetInfo[];
}

export function RemoteNodeRemoteStorageTargetsList({
	errorMessage,
	loading,
	pendingDeleteTargetKey,
	onCancelDelete,
	onConfirmDeleteTarget,
	onRequestDeleteTarget,
	onEditTarget,
	targets,
}: RemoteNodeRemoteStorageTargetsListProps) {
	const { t } = useTranslation("admin");

	return (
		<div className="mt-4 space-y-3">
			{errorMessage ? null : loading ? (
				<div className="rounded-2xl border border-border/70 bg-muted/10 p-4 text-sm text-muted-foreground">
					<span className="inline-flex items-center gap-2">
						<Icon name="Spinner" className="size-4 animate-spin" />
						{t("core:loading")}
					</span>
				</div>
			) : targets.length === 0 ? (
				<div className="rounded-2xl border border-dashed border-border/70 bg-muted/10 p-4">
					<p className="text-sm font-medium text-foreground">
						{t("remote_node_ingress_profiles_empty")}
					</p>
					<p className="mt-1 text-sm text-muted-foreground">
						{t("remote_node_ingress_profiles_empty_desc")}
					</p>
				</div>
			) : (
				targets.map((target) => (
					<RemoteNodeRemoteStorageTargetCard
						key={target.target_key}
						deleteConfirming={pendingDeleteTargetKey === target.target_key}
						onCancelDelete={onCancelDelete}
						onConfirmDelete={() => onConfirmDeleteTarget(target)}
						onRequestDelete={() => onRequestDeleteTarget(target)}
						onEdit={() => onEditTarget(target)}
						target={target}
					/>
				))
			)}
		</div>
	);
}

interface RemoteNodeRemoteStorageTargetCardProps {
	deleteConfirming: boolean;
	onCancelDelete: () => void;
	onConfirmDelete: () => void;
	onRequestDelete: () => void;
	onEdit: () => void;
	target: RemoteStorageTargetInfo;
}

function RemoteNodeRemoteStorageTargetCard({
	deleteConfirming,
	onCancelDelete,
	onConfirmDelete,
	onRequestDelete,
	onEdit,
	target,
}: RemoteNodeRemoteStorageTargetCardProps) {
	const { t } = useTranslation("admin");
	const status = getRemoteNodeRemoteStorageTargetProfileStatus(target);

	return (
		<article className="rounded-2xl border border-border/70 bg-muted/10 p-4">
			<div className="flex flex-wrap items-start justify-between gap-3">
				<div className="min-w-0">
					<div className="flex flex-wrap items-center gap-2">
						<h4 className="truncate text-sm font-semibold text-foreground">
							{target.name}
						</h4>
						<Badge
							variant="outline"
							className={getRemoteNodeRemoteStorageTargetDriverBadgeTone(
								target.driver_type,
							)}
						>
							{target.driver_type === "s3"
								? t("remote_node_ingress_profile_driver_s3")
								: t("remote_node_ingress_profile_driver_local")}
						</Badge>
						{target.is_default ? (
							<Badge
								variant="outline"
								className="border-blue-500/60 bg-blue-500/10 text-blue-700 dark:text-blue-300"
							>
								{t("remote_node_ingress_profile_default")}
							</Badge>
						) : null}
						<Badge variant="outline" className={status.toneClass}>
							{t(status.labelKey)}
						</Badge>
					</div>
					<p className="mt-1 break-all font-mono text-xs text-muted-foreground">
						{target.target_key}
					</p>
				</div>

				<div className="flex shrink-0 gap-1">
					<Button
						type="button"
						variant="ghost"
						size="icon"
						className={ADMIN_ICON_BUTTON_CLASS}
						onClick={onEdit}
						aria-label={t("core:edit")}
						title={t("core:edit")}
					>
						<Icon name="PencilSimple" className="size-3.5" />
					</Button>
					{deleteConfirming ? (
						<div className="flex items-center gap-1 duration-150 animate-in fade-in zoom-in-95 motion-reduce:animate-none">
							<Button
								type="button"
								variant="destructive"
								size="sm"
								onClick={onConfirmDelete}
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
						<Button
							type="button"
							variant="ghost"
							size="icon"
							className={`${ADMIN_ICON_BUTTON_CLASS} text-destructive`}
							onClick={onRequestDelete}
							aria-label={t("core:delete")}
							title={t("core:delete")}
						>
							<Icon name="Trash" className="size-3.5" />
						</Button>
					)}
				</div>
			</div>

			{deleteConfirming ? (
				<div className="mt-3 rounded-xl border border-destructive/30 bg-destructive/5 p-3 text-sm duration-150 animate-in fade-in slide-in-from-top-1 motion-reduce:animate-none">
					<p className="font-medium text-destructive">
						{t("remote_node_ingress_profile_delete_title", {
							name: target.name,
						})}
					</p>
					<p className="mt-1 text-muted-foreground">
						{t("remote_node_ingress_profile_delete_desc")}
					</p>
				</div>
			) : null}

			<dl className="mt-4 grid gap-3 text-sm md:grid-cols-2">
				<div>
					<dt className="text-[11px] font-medium uppercase tracking-[0.14em] text-muted-foreground">
						{t("base_path")}
					</dt>
					<dd className="mt-1 break-all font-medium">
						{target.base_path || "."}
					</dd>
				</div>
				<div>
					<dt className="text-[11px] font-medium uppercase tracking-[0.14em] text-muted-foreground">
						{t("max_file_size")}
					</dt>
					<dd className="mt-1 font-medium">
						{target.max_file_size > 0
							? formatBytes(target.max_file_size)
							: t("core:unlimited")}
					</dd>
				</div>
				{target.driver_type === "s3" ? (
					<>
						<div>
							<dt className="text-[11px] font-medium uppercase tracking-[0.14em] text-muted-foreground">
								{t("endpoint")}
							</dt>
							<dd className="mt-1 break-all font-medium">{target.endpoint}</dd>
						</div>
						<div>
							<dt className="text-[11px] font-medium uppercase tracking-[0.14em] text-muted-foreground">
								{t("bucket")}
							</dt>
							<dd className="mt-1 break-all font-medium">{target.bucket}</dd>
						</div>
					</>
				) : null}
				<div>
					<dt className="text-[11px] font-medium uppercase tracking-[0.14em] text-muted-foreground">
						{t("remote_node_ingress_profile_revision")}
					</dt>
					<dd className="mt-1 font-medium">
						{target.applied_revision} / {target.desired_revision}
					</dd>
				</div>
				<div>
					<dt className="text-[11px] font-medium uppercase tracking-[0.14em] text-muted-foreground">
						{t("core:updated_at")}
					</dt>
					<dd className="mt-1 font-medium">
						{formatDateTime(target.updated_at)}
					</dd>
				</div>
			</dl>

			<div className="mt-4 rounded-2xl border border-border/70 bg-background/70 p-3">
				<div className="text-[11px] font-medium uppercase tracking-[0.14em] text-muted-foreground">
					{t("remote_node_ingress_profile_last_error")}
				</div>
				<div className="mt-1 break-all text-sm">
					{target.last_error ||
						t("remote_node_ingress_profile_last_error_empty")}
				</div>
			</div>
		</article>
	);
}
