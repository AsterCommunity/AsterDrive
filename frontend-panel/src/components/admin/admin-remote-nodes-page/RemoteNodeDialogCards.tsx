import { useTranslation } from "react-i18next";
import { Badge } from "@/components/ui/badge";
import { buttonVariants } from "@/components/ui/buttonVariants";
import { Icon } from "@/components/ui/icon";
import { ADMIN_CONTROL_HEIGHT_CLASS } from "@/lib/constants";
import { cn } from "@/lib/utils";
import type { RemoteNodeInfo } from "@/types/api";
import type { RemoteNodeFormData } from "../remoteNodeDialogShared";
import type { RemoteNodeSummaryItem } from "./RemoteNodeDialogTypes";
import {
	formatLastChecked,
	getRemoteNodeEnrollmentStatusLabel,
	getRemoteNodeEnrollmentStatusTone,
} from "./shared";

export const DOCKER_FOLLOWER_DOCS_URL =
	"https://drive.astercosm.com/deployment/docker-follower";

export function RemoteNodeSectionIntro({
	description,
	title,
}: {
	description: string;
	title: string;
}) {
	return (
		<div className="mb-5">
			<h3 className="text-base font-semibold text-foreground">{title}</h3>
			<p className="mt-1 text-sm text-muted-foreground">{description}</p>
		</div>
	);
}

interface RemoteNodeSummaryCardProps {
	description: string;
	editingNode: RemoteNodeInfo | null;
	enabledToneClass: string;
	form: RemoteNodeFormData;
	modeToneClass: string;
	remoteNodeModeLabel: string;
	summaryItems: RemoteNodeSummaryItem[];
}

export function RemoteNodeSummaryCard({
	description,
	editingNode,
	enabledToneClass,
	form,
	modeToneClass,
	remoteNodeModeLabel,
	summaryItems,
}: RemoteNodeSummaryCardProps) {
	const { t } = useTranslation("admin");

	return (
		<section className="rounded-3xl border border-border/70 bg-muted/20 p-5">
			<div className="flex items-center gap-3">
				<div className="flex size-14 items-center justify-center rounded-2xl bg-white shadow-sm ring-1 ring-black/5">
					<img
						src="/static/asterdrive/asterdrive-dark.svg"
						alt=""
						className="max-h-7 w-auto object-contain"
					/>
				</div>
				<div className="min-w-0">
					<p className="text-xs font-medium uppercase tracking-[0.18em] text-muted-foreground">
						{t("remote_node_summary_title")}
					</p>
					<h3 className="mt-1 truncate text-base font-semibold">
						{form.name || t("new_remote_node")}
					</h3>
				</div>
			</div>
			<p className="mt-4 text-sm leading-6 text-muted-foreground">
				{description}
			</p>
			<div className="mt-4 flex flex-wrap gap-2">
				<Badge variant="outline" className={modeToneClass}>
					{remoteNodeModeLabel}
				</Badge>
				{editingNode ? (
					<Badge
						variant="outline"
						className={getRemoteNodeEnrollmentStatusTone(
							editingNode.enrollment_status,
						)}
					>
						{getRemoteNodeEnrollmentStatusLabel(
							t,
							editingNode.enrollment_status,
						)}
					</Badge>
				) : null}
				<Badge variant="outline" className={enabledToneClass}>
					{form.is_enabled
						? t("remote_node_status_enabled")
						: t("remote_node_status_disabled")}
				</Badge>
			</div>
			<dl className="mt-4 space-y-3 text-sm">
				{summaryItems.map((item) => (
					<div key={item.label}>
						<dt className="text-[11px] font-medium uppercase tracking-[0.14em] text-muted-foreground">
							{item.label}
						</dt>
						<dd className="mt-1 break-all font-medium">{item.value}</dd>
					</div>
				))}
			</dl>
		</section>
	);
}

export function RemoteNodeDocsCard() {
	const { t } = useTranslation("admin");

	return (
		<section className="rounded-3xl border border-border/70 bg-background/85 p-5">
			<div className="flex items-start gap-3">
				<div className="flex size-10 shrink-0 items-center justify-center rounded-2xl bg-muted/20 text-primary">
					<Icon name="Docker" className="size-5" />
				</div>
				<div className="min-w-0">
					<h3 className="text-sm font-semibold">
						{t("remote_node_wizard_docs_title")}
					</h3>
					<p className="mt-1 text-xs leading-5 text-muted-foreground">
						{t("remote_node_wizard_docs_desc")}
					</p>
				</div>
			</div>
			<a
				href={DOCKER_FOLLOWER_DOCS_URL}
				target="_blank"
				rel="noreferrer"
				className={cn(
					buttonVariants({ variant: "outline", size: "sm" }),
					`${ADMIN_CONTROL_HEIGHT_CLASS} mt-4 w-full justify-between rounded-xl`,
				)}
			>
				<span className="inline-flex items-center gap-2">
					<Icon name="Globe" className="size-4" />
					{t("remote_node_wizard_docs_link")}
				</span>
				<Icon
					name="ArrowSquareOut"
					className="size-3.5 text-muted-foreground"
				/>
			</a>
		</section>
	);
}

export function RemoteNodeDiagnosticsCard({
	editingNode,
}: {
	editingNode: RemoteNodeInfo;
}) {
	const { t } = useTranslation("admin");

	return (
		<section className="rounded-3xl border border-border/70 bg-background/85 p-5">
			<h3 className="text-sm font-semibold">
				{t("remote_node_diagnostics_title")}
			</h3>
			<p className="mt-1 text-xs text-muted-foreground">
				{t("remote_node_diagnostics_desc")}
			</p>
			<dl className="mt-4 space-y-3 text-sm">
				<div>
					<dt className="text-[11px] font-medium uppercase tracking-[0.14em] text-muted-foreground">
						{t("remote_node_enrollment_status")}
					</dt>
					<dd className="mt-1">
						<Badge
							variant="outline"
							className={getRemoteNodeEnrollmentStatusTone(
								editingNode.enrollment_status,
							)}
						>
							{getRemoteNodeEnrollmentStatusLabel(
								t,
								editingNode.enrollment_status,
							)}
						</Badge>
					</dd>
				</div>
				<div>
					<dt className="text-[11px] font-medium uppercase tracking-[0.14em] text-muted-foreground">
						{t("remote_node_last_checked")}
					</dt>
					<dd className="mt-1 break-all font-medium">
						{formatLastChecked(t, editingNode.last_checked_at)}
					</dd>
				</div>
				<div>
					<dt className="text-[11px] font-medium uppercase tracking-[0.14em] text-muted-foreground">
						{t("remote_node_last_error")}
					</dt>
					<dd className="mt-1 break-all font-medium">
						{editingNode.last_error || t("remote_node_last_error_empty")}
					</dd>
				</div>
				<div>
					<dt className="text-[11px] font-medium uppercase tracking-[0.14em] text-muted-foreground">
						{t("remote_node_capabilities")}
					</dt>
					<dd className="mt-1 space-y-1 text-xs text-muted-foreground">
						<div>
							{t("remote_node_protocol_version")}:{" "}
							{editingNode.capabilities.protocol_version}
						</div>
						<div>
							{t("remote_node_supports_list")}:{" "}
							{String(editingNode.capabilities.supports_list)}
						</div>
						<div>
							{t("remote_node_supports_range_read")}:{" "}
							{String(editingNode.capabilities.supports_range_read)}
						</div>
						<div>
							{t("remote_node_supports_stream_upload")}:{" "}
							{String(editingNode.capabilities.supports_stream_upload)}
						</div>
					</dd>
				</div>
			</dl>
		</section>
	);
}
