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
	ADMIN_ICON_BUTTON_CLASS,
	ADMIN_TABLE_ACTIONS_WIDTH_CLASS,
} from "@/lib/constants";
import type { SortOrder } from "@/lib/pagination";
import type { AdminPolicySortBy } from "@/types/adminSort";
import type { StoragePolicy } from "@/types/api";
import {
	getPolicyDriverBadgeClass,
	PROTECTED_POLICY_ID,
} from "./policyPresentation";

interface PoliciesTableProps {
	deletingPolicyId: number | null;
	loading: boolean;
	onDeletePolicy: (policyId: number) => void;
	onEditPolicy: (policy: StoragePolicy) => void;
	policies: StoragePolicy[];
	remoteNodeNameById: Map<number, string>;
	sortBy: AdminPolicySortBy;
	sortOrder: SortOrder;
	onSortChange: (sortBy: AdminPolicySortBy, sortOrder: SortOrder) => void;
}

export function PoliciesTable({
	deletingPolicyId,
	loading,
	onDeletePolicy,
	onEditPolicy,
	onSortChange,
	policies,
	remoteNodeNameById,
	sortBy,
	sortOrder,
}: PoliciesTableProps) {
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
						sortKey="driver_type"
						sortBy={sortBy}
						sortOrder={sortOrder}
						onSortChange={onSortChange}
					>
						{t("driver_type")}
					</AdminSortableTableHead>
					<AdminSortableTableHead
						sortKey="endpoint"
						sortBy={sortBy}
						sortOrder={sortOrder}
						onSortChange={onSortChange}
					>
						{t("endpoint_path")}
					</AdminSortableTableHead>
					<AdminSortableTableHead
						sortKey="bucket"
						sortBy={sortBy}
						sortOrder={sortOrder}
						onSortChange={onSortChange}
					>
						{t("bucket")}
					</AdminSortableTableHead>
					<AdminSortableTableHead
						className="w-20"
						sortKey="is_default"
						sortBy={sortBy}
						sortOrder={sortOrder}
						onSortChange={onSortChange}
					>
						{t("is_default")}
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
			loading={loading}
			items={policies}
			columns={7}
			rows={6}
			emptyTitle={t("no_policies")}
			emptyDescription={t("no_policies_desc")}
			headerRow={headerRow}
			renderRow={(policy) => {
				const isDeleting = deletingPolicyId === policy.id;
				const deleteLabel = isDeleting
					? t("policy_deleting")
					: t("delete_policy");

				return (
					<TableRow
						key={policy.id}
						className={ADMIN_INTERACTIVE_TABLE_ROW_CLASS}
						onClick={() => {
							if (!isDeleting) onEditPolicy(policy);
						}}
						onKeyDown={(event) => {
							if (event.key === "Enter" || event.key === " ") {
								event.preventDefault();
								if (!isDeleting) onEditPolicy(policy);
							}
						}}
						tabIndex={0}
					>
						<TableCell>
							<div className={ADMIN_TABLE_TEXT_CELL_CLASS}>
								<span className={ADMIN_TABLE_MONO_TEXT_CLASS}>{policy.id}</span>
							</div>
						</TableCell>
						<TableCell>
							<div className={ADMIN_TABLE_TEXT_CELL_CLASS}>
								<div className="min-w-0">
									<div className="truncate font-medium text-foreground">
										{policy.name}
									</div>
								</div>
							</div>
						</TableCell>
						<TableCell>
							<div className={ADMIN_TABLE_BADGE_CELL_CLASS}>
								<Badge
									variant="outline"
									className={getPolicyDriverBadgeClass(policy.driver_type)}
								>
									{policy.driver_type === "local"
										? t("driver_type_local")
										: policy.driver_type === "remote"
											? t("driver_type_remote")
											: policy.driver_type === "tencent_cos"
												? t("driver_type_tencent_cos")
												: t("driver_type_s3")}
								</Badge>
							</div>
						</TableCell>
						<TableCell>
							<div className={ADMIN_TABLE_TEXT_CELL_CLASS}>
								<span className="truncate text-xs font-mono text-muted-foreground">
									{policy.driver_type === "local"
										? policy.base_path || "./data"
										: policy.driver_type === "remote"
											? policy.base_path || t("core:root")
											: policy.endpoint}
								</span>
							</div>
						</TableCell>
						<TableCell>
							<div className={ADMIN_TABLE_TEXT_CELL_CLASS}>
								<span className="truncate text-xs text-muted-foreground">
									{policy.driver_type === "remote"
										? policy.remote_node_id != null
											? (remoteNodeNameById.get(policy.remote_node_id) ??
												`#${policy.remote_node_id}`)
											: "-"
										: policy.bucket || "-"}
								</span>
							</div>
						</TableCell>
						<TableCell>
							<div className={ADMIN_TABLE_BADGE_CELL_CLASS}>
								{policy.is_default ? (
									<Badge className="bg-blue-100 border-blue-300 text-blue-700 dark:border-blue-700 dark:bg-blue-900 dark:text-blue-300">
										{t("is_default")}
									</Badge>
								) : (
									<span className="text-xs text-muted-foreground">-</span>
								)}
							</div>
						</TableCell>
						<TableCell
							onClick={(event) => event.stopPropagation()}
							onKeyDown={(event) => event.stopPropagation()}
						>
							<div className="flex justify-end">
								<Button
									variant="ghost"
									size="icon"
									className={`${ADMIN_ICON_BUTTON_CLASS} text-destructive`}
									onClick={() => onDeletePolicy(policy.id)}
									aria-label={deleteLabel}
									title={
										policy.id === PROTECTED_POLICY_ID
											? t("initial_policy_delete_blocked")
											: deleteLabel
									}
									disabled={policy.id === PROTECTED_POLICY_ID || isDeleting}
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
