import { useMemo } from "react";
import { useTranslation } from "react-i18next";
import {
	ADMIN_INTERACTIVE_TABLE_ROW_CLASS,
	ADMIN_TABLE_BADGE_CELL_CLASS,
	ADMIN_TABLE_MONO_TEXT_CLASS,
	ADMIN_TABLE_TEXT_CELL_CLASS,
	AdminSortableTableHead,
	AdminTableCell as TableCell,
	AdminTableHead as TableHead,
	AdminTableHeader as TableHeader,
	AdminTableRow as TableRow,
} from "@/components/common/AdminTable";
import { AdminTableList } from "@/components/common/AdminTableList";
import { Badge } from "@/components/ui/badge";
import { Button } from "@/components/ui/button";
import { Icon } from "@/components/ui/icon";
import {
	Tooltip,
	TooltipContent,
	TooltipProvider,
	TooltipTrigger,
} from "@/components/ui/tooltip";
import {
	ADMIN_ICON_BUTTON_CLASS,
	ADMIN_TABLE_ACTIONS_WIDTH_CLASS,
} from "@/lib/constants";
import type { SortOrder } from "@/lib/pagination";
import { cn } from "@/lib/utils";
import type { AdminRemoteNodeSortBy } from "@/types/adminSort";
import type { RemoteNodeInfo } from "@/types/api";
import {
	formatLastChecked,
	getRemoteNodeEnrollmentStatusLabel,
	getRemoteNodeEnrollmentStatusTone,
	getRemoteNodeStatusLabel,
	getRemoteNodeStatusTone,
	getRemoteNodeTransportBadge,
	getRemoteNodeTransportLabel,
	getRemoteNodeTransportTone,
	getRemoteNodeTunnelLabel,
	getRemoteNodeTunnelTone,
	hasCompletedRemoteNodeEnrollment,
} from "./shared";

interface RemoteNodesTableProps {
	deletingRemoteNodeId: number | null;
	generatingEnrollmentId: number | null;
	items: RemoteNodeInfo[];
	loading: boolean;
	onEdit: (node: RemoteNodeInfo) => void;
	onGenerateEnrollmentCommand: (node: RemoteNodeInfo) => void;
	onRequestDelete: (id: number) => void;
	sortBy: AdminRemoteNodeSortBy;
	sortOrder: SortOrder;
	onSortChange: (sortBy: AdminRemoteNodeSortBy, sortOrder: SortOrder) => void;
}

export function RemoteNodesTable({
	deletingRemoteNodeId,
	generatingEnrollmentId,
	items,
	loading,
	onEdit,
	onGenerateEnrollmentCommand,
	onRequestDelete,
	onSortChange,
	sortBy,
	sortOrder,
}: RemoteNodesTableProps) {
	const { t } = useTranslation("admin");
	const headerRow = useMemo(
		() => (
			<TableHeader>
				<TableRow>
					<AdminSortableTableHead
						className="w-16"
						sortKey="id"
						sortBy={sortBy}
						sortOrder={sortOrder}
						onSortChange={onSortChange}
					>
						{t("id")}
					</AdminSortableTableHead>
					<AdminSortableTableHead
						sortKey="name"
						sortBy={sortBy}
						sortOrder={sortOrder}
						onSortChange={onSortChange}
					>
						{t("core:name")}
					</AdminSortableTableHead>
					<AdminSortableTableHead
						sortKey="base_url"
						sortBy={sortBy}
						sortOrder={sortOrder}
						onSortChange={onSortChange}
					>
						{t("base_url")}
					</AdminSortableTableHead>
					<TableHead>{t("remote_node_transport_mode")}</TableHead>
					<AdminSortableTableHead
						sortKey="last_checked_at"
						sortBy={sortBy}
						sortOrder={sortOrder}
						onSortChange={onSortChange}
					>
						{t("remote_node_status")}
					</AdminSortableTableHead>
					<TableHead className={ADMIN_TABLE_ACTIONS_WIDTH_CLASS}>
						{t("core:actions")}
					</TableHead>
				</TableRow>
			</TableHeader>
		),
		[onSortChange, sortBy, sortOrder, t],
	);

	return (
		<AdminTableList
			frameless
			loading={loading}
			items={items}
			columns={6}
			rows={6}
			emptyTitle={t("no_remote_nodes")}
			emptyDescription={t("no_remote_nodes_desc")}
			headerRow={headerRow}
			renderRow={(node) => {
				const isDeleting = deletingRemoteNodeId === node.id;
				const enrollmentCompleted = hasCompletedRemoteNodeEnrollment(node);
				const generateEnrollmentDisabled =
					generatingEnrollmentId === node.id ||
					enrollmentCompleted ||
					isDeleting;
				const generateEnrollmentLabel = enrollmentCompleted
					? t("remote_node_enrollment_completed_action_disabled")
					: t("remote_node_generate_enrollment_command");
				const deleteLabel = isDeleting
					? t("remote_node_deleting")
					: t("delete_remote_node");
				const transportMode = node.transport_mode ?? "direct";
				const transportBadge = getRemoteNodeTransportBadge(t, transportMode);

				return (
					<TableRow
						key={node.id}
						className={ADMIN_INTERACTIVE_TABLE_ROW_CLASS}
						onClick={() => {
							if (!isDeleting) onEdit(node);
						}}
						onKeyDown={(event) => {
							if (event.key === "Enter" || event.key === " ") {
								event.preventDefault();
								if (!isDeleting) onEdit(node);
							}
						}}
						tabIndex={0}
					>
						<TableCell>
							<div className={ADMIN_TABLE_TEXT_CELL_CLASS}>
								<span className={ADMIN_TABLE_MONO_TEXT_CLASS}>{node.id}</span>
							</div>
						</TableCell>
						<TableCell>
							<div className={ADMIN_TABLE_TEXT_CELL_CLASS}>
								<div className="min-w-0">
									<div className="truncate font-medium text-foreground">
										{node.name}
									</div>
								</div>
							</div>
						</TableCell>
						<TableCell>
							<div className={ADMIN_TABLE_TEXT_CELL_CLASS}>
								<span className="truncate text-xs font-mono text-muted-foreground">
									{node.base_url || t("remote_node_base_url_empty")}
								</span>
							</div>
						</TableCell>
						<TableCell>
							<div className={ADMIN_TABLE_BADGE_CELL_CLASS}>
								<div className="space-y-2">
									<div className="flex flex-wrap gap-1.5">
										<Badge
											variant="outline"
											className={getRemoteNodeTransportTone(transportMode)}
										>
											{getRemoteNodeTransportLabel(t, transportMode)}
											{transportBadge ? (
												<span className="ml-1.5 rounded border border-amber-500/40 bg-amber-500/10 px-1 py-0.5 text-[10px] font-medium text-amber-700 dark:text-amber-300">
													{transportBadge}
												</span>
											) : null}
										</Badge>
										<Badge
											variant="outline"
											className={getRemoteNodeTunnelTone(node)}
										>
											{getRemoteNodeTunnelLabel(t, node)}
										</Badge>
									</div>
									{node.tunnel?.last_error ? (
										<div className="line-clamp-2 text-xs text-muted-foreground">
											{node.tunnel.last_error}
										</div>
									) : null}
								</div>
							</div>
						</TableCell>
						<TableCell>
							<div className={ADMIN_TABLE_BADGE_CELL_CLASS}>
								<div className="space-y-2">
									<div className="flex flex-wrap gap-1.5">
										<Badge
											variant="outline"
											className={getRemoteNodeStatusTone(node)}
										>
											{getRemoteNodeStatusLabel(t, node)}
										</Badge>
										<Badge
											variant="outline"
											className={getRemoteNodeEnrollmentStatusTone(
												node.enrollment_status,
											)}
										>
											{getRemoteNodeEnrollmentStatusLabel(
												t,
												node.enrollment_status,
											)}
										</Badge>
									</div>
									<div className="text-xs text-muted-foreground">
										{formatLastChecked(t, node.last_checked_at)}
									</div>
								</div>
							</div>
						</TableCell>
						<TableCell
							className={cn(
								ADMIN_TABLE_ACTIONS_WIDTH_CLASS,
								"whitespace-nowrap",
							)}
							onClick={(event) => event.stopPropagation()}
							onKeyDown={(event) => event.stopPropagation()}
						>
							<div className="flex w-full justify-end gap-1">
								<TooltipProvider>
									<Tooltip>
										<TooltipTrigger
											render={<span className="inline-flex size-8 shrink-0" />}
										>
											<Button
												variant="ghost"
												size="icon"
												className={ADMIN_ICON_BUTTON_CLASS}
												onClick={() => onGenerateEnrollmentCommand(node)}
												disabled={generateEnrollmentDisabled}
												aria-label={generateEnrollmentLabel}
												title={generateEnrollmentLabel}
											>
												<Icon
													name={
														generatingEnrollmentId === node.id
															? "Spinner"
															: "ClipboardText"
													}
													className={cn(
														"size-3.5",
														generatingEnrollmentId === node.id &&
															"animate-spin",
													)}
												/>
											</Button>
										</TooltipTrigger>
										{enrollmentCompleted ? (
											<TooltipContent>{generateEnrollmentLabel}</TooltipContent>
										) : null}
									</Tooltip>
								</TooltipProvider>
								<Button
									variant="ghost"
									size="icon"
									className={`${ADMIN_ICON_BUTTON_CLASS} text-destructive`}
									onClick={() => onRequestDelete(node.id)}
									aria-label={deleteLabel}
									title={deleteLabel}
									disabled={isDeleting}
								>
									<Icon
										name={isDeleting ? "Spinner" : "Trash"}
										className={`size-3.5 ${isDeleting ? "animate-spin" : ""}`}
									/>
								</Button>
							</div>
						</TableCell>
					</TableRow>
				);
			}}
		/>
	);
}
