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
import {
	ADMIN_ICON_BUTTON_CLASS,
	ADMIN_TABLE_ACTIONS_WIDTH_CLASS,
} from "@/lib/constants";
import type { StoragePolicy } from "@/types/api";
import {
	getPolicyDriverBadgeClass,
	PROTECTED_POLICY_ID,
} from "./policyPresentation";

interface PoliciesTableProps {
	loading: boolean;
	onDeletePolicy: (policyId: number) => void;
	onEditPolicy: (policy: StoragePolicy) => void;
	policies: StoragePolicy[];
	remoteNodeNameById: Map<number, string>;
}

export function PoliciesTable({
	loading,
	onDeletePolicy,
	onEditPolicy,
	policies,
	remoteNodeNameById,
}: PoliciesTableProps) {
	const { t } = useTranslation("admin");

	return (
		<AdminTableList
			loading={loading}
			items={policies}
			columns={7}
			rows={6}
			emptyTitle={t("no_policies")}
			emptyDescription={t("no_policies_desc")}
			headerRow={
				<TableHeader>
					<TableRow>
						<TableHead className="w-16">{t("id")}</TableHead>
						<TableHead>{t("core:name")}</TableHead>
						<TableHead>{t("driver_type")}</TableHead>
						<TableHead>{t("endpoint_path")}</TableHead>
						<TableHead>{t("bucket")}</TableHead>
						<TableHead className="w-20">{t("is_default")}</TableHead>
						<TableHead className={ADMIN_TABLE_ACTIONS_WIDTH_CLASS}>
							{t("core:actions")}
						</TableHead>
					</TableRow>
				</TableHeader>
			}
			renderRow={(policy) => (
				<TableRow
					key={policy.id}
					className={ADMIN_INTERACTIVE_TABLE_ROW_CLASS}
					onClick={() => onEditPolicy(policy)}
					onKeyDown={(event) => {
						if (event.key === "Enter" || event.key === " ") {
							event.preventDefault();
							onEditPolicy(policy);
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
								aria-label={t("delete_policy")}
								title={
									policy.id === PROTECTED_POLICY_ID
										? t("initial_policy_delete_blocked")
										: t("delete_policy")
								}
								disabled={policy.id === PROTECTED_POLICY_ID}
							>
								<Icon name="Trash" className="h-3.5 w-3.5" />
							</Button>
						</div>
					</TableCell>
				</TableRow>
			)}
		/>
	);
}
