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
import { translateStorageConnectorMessage } from "@/lib/adminStorageConnectorLocalizations";
import {
	ADMIN_ICON_BUTTON_CLASS,
	ADMIN_TABLE_ACTIONS_WIDTH_CLASS,
} from "@/lib/constants";
import type { SortOrder } from "@/lib/pagination";
import type { AdminPolicySortBy } from "@/types/adminSort";
import type { StorageConnectorDescriptor, StoragePolicy } from "@/types/api";
import { policyConnectorSelection } from "../storage-policy-dialog/connectionNormalization";
import type { ConnectorFormValue } from "../storage-policy-dialog/formTypes";
import { getStorageConnectorBadgePresentation } from "./policyPresentation";

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
	const descriptorByConnectorId = useMemo(
		() =>
			new Map(
				storageDriverDescriptors.map((descriptor) => [
					descriptor.connector_id,
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
						sortKey="connector_id"
						sortBy={sortBy}
						sortOrder={sortOrder}
						onSortChange={onSortChange}
					>
						{t("driver_type")}
					</AdminSortableTableHead>
					<TableHead>{t("policy_connector_configuration")}</TableHead>
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
			frameless
			loading={loading}
			items={policies}
			columns={6}
			rows={6}
			emptyTitle={t("no_policies")}
			emptyDescription={t("no_policies_desc")}
			headerRow={headerRow}
			renderRow={(policy) => {
				const isDeleting = deletingPolicyId === policy.id;
				const deleteLabel = isDeleting
					? t("policy_deleting")
					: t("delete_policy");
				const selection = policyConnectorSelection(policy);
				const descriptor = descriptorByConnectorId.get(policy.connector_id);
				const badgePresentation = getStorageConnectorBadgePresentation(
					descriptor?.ui.badge_rgb,
				);
				const connectorT = (key: string) =>
					translateStorageConnectorMessage(t, descriptor?.connector_id, key);
				const connectorLabel = descriptor?.ui
					? connectorT(descriptor.ui.label_key)
					: policy.connector_id;
				const configurationSummary = buildConfigurationSummary(
					descriptor,
					selection.connector_config_values,
					remoteNodeNameById,
					t,
					connectorT,
				);

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
									className={badgePresentation.className}
									style={badgePresentation.style}
								>
									{connectorLabel}
								</Badge>
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

function buildConfigurationSummary(
	descriptor: StorageConnectorDescriptor | undefined,
	values: Record<string, ConnectorFormValue>,
	remoteNodeNameById: Map<number, string>,
	t: (key: string) => string,
	connectorT: (key: string) => string,
) {
	const parts = (descriptor?.fields ?? [])
		.filter(
			(field) =>
				field.scope === "connector_config" &&
				!field.secret &&
				values[field.name] !== undefined &&
				values[field.name] !== null &&
				values[field.name] !== "",
		)
		.slice(0, 3)
		.map((field) => {
			const value = values[field.name];
			const displayed =
				field.select?.data_source === "remote_nodes" &&
				typeof value === "number"
					? (remoteNodeNameById.get(value) ?? `#${value}`)
					: field.select?.options?.find((option) => option.value === value)
						? connectorT(
								field.select.options.find((option) => option.value === value)
									?.label_key ?? field.label_key,
							)
						: scalarDisplay(value, t);
			return `${connectorT(field.label_key)}: ${displayed}`;
		});
	return parts.length > 0 ? parts.join(" · ") : "-";
}

function scalarDisplay(value: ConnectorFormValue, t: (key: string) => string) {
	if (typeof value === "boolean") {
		return value ? t("core:yes") : t("core:no");
	}
	return String(value);
}
