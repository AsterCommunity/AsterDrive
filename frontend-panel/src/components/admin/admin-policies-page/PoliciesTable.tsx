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
import type { StorageConnectorDescriptor, StoragePolicy } from "@/types/api";
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
	storageDriverDescriptors: StorageConnectorDescriptor[];
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
	storageDriverDescriptors,
}: PoliciesTableProps) {
	const { t } = useTranslation("admin");
	const descriptorByDriverType = useMemo(
		() =>
			new Map(
				storageDriverDescriptors.map((descriptor) => [
					descriptor.driver_type,
					descriptor,
				]),
			),
		[storageDriverDescriptors],
	);
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
				const descriptor = descriptorByDriverType.get(policy.driver_type);
				const driverLabel = descriptor?.ui
					? t(descriptor.ui.label_key)
					: policy.driver_type;
				const basePathEmptyDisplay = descriptor?.ui
					? translateStorageConnectorUiValue(
							descriptor.ui.base_path_empty_display,
							t,
						)
					: t("core:root");
				const endpointOrPath = descriptor?.fields.some(
					(field) => field.scope === "connection" && field.name === "endpoint",
				)
					? policy.endpoint
					: policy.base_path || basePathEmptyDisplay;
				const bucketOrBinding = descriptor?.fields.some(
					(field) =>
						field.scope === "remote_node_binding" &&
						field.name === "remote_node_id",
				)
					? policy.remote_node_id != null
						? (remoteNodeNameById.get(policy.remote_node_id) ??
							`#${policy.remote_node_id}`)
						: "-"
					: policy.bucket || "-";

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
									{driverLabel}
								</Badge>
							</div>
						</TableCell>
						<TableCell>
							<div className={ADMIN_TABLE_TEXT_CELL_CLASS}>
								<span className="truncate text-xs font-mono text-muted-foreground">
									{endpointOrPath}
								</span>
							</div>
						</TableCell>
						<TableCell>
							<div className={ADMIN_TABLE_TEXT_CELL_CLASS}>
								<span className="truncate text-xs text-muted-foreground">
									{bucketOrBinding}
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

function translateStorageConnectorUiValue(
	value: string,
	t: (key: string) => string,
) {
	return value.includes(":") ? t(value) : value;
}
