import { Input } from "@/components/ui/input";
import { Label } from "@/components/ui/label";
import {
	Select,
	SelectContent,
	SelectItem,
	SelectTrigger,
	SelectValue,
} from "@/components/ui/select";
import { ADMIN_CONTROL_HEIGHT_CLASS } from "@/lib/constants";
import {
	connectorStringValue,
	updatedConnectorConfigValues,
} from "./formTypes";
import {
	getDefaultTenant,
	getTenantMode,
	ONE_DRIVE_AUTO_TENANT_MODE,
} from "./onedriveFieldUtils";
import type { SharedFieldProps, Translate } from "./StoragePolicyFieldTypes";

export function OneDriveTargetFields({
	accountModeOptions,
	form,
	onFieldChange,
	t,
}: SharedFieldProps & {
	accountModeOptions: Array<{
		label: string;
		value: string;
	}>;
	t: Translate;
}) {
	const accountMode = connectorStringValue(
		form,
		"account_mode",
		"work_or_school",
	);
	const setConnectorValue = (name: string, value: string) =>
		onFieldChange(
			"connector_config_values",
			updatedConnectorConfigValues(form, name, value),
		);
	return (
		<>
			<div className="space-y-2">
				<Label htmlFor="onedrive_account_mode">
					{t("onedrive_account_mode")}
				</Label>
				<Select
					items={accountModeOptions}
					value={accountMode}
					onValueChange={(value) => {
						const nextMode = value ?? "work_or_school";
						const tenantMode = getTenantMode(form);
						setConnectorValue("account_mode", nextMode);
						if (tenantMode === ONE_DRIVE_AUTO_TENANT_MODE) {
							setConnectorValue("tenant", getDefaultTenant(nextMode));
						}
					}}
				>
					<SelectTrigger id="onedrive_account_mode">
						<SelectValue />
					</SelectTrigger>
					<SelectContent>
						{accountModeOptions.map((option) => (
							<SelectItem key={option.value} value={option.value}>
								{option.label}
							</SelectItem>
						))}
					</SelectContent>
				</Select>
				<p className="text-xs leading-5 text-muted-foreground">
					{t("onedrive_account_mode_desc")}
				</p>
			</div>
			<div className="space-y-2">
				<Label htmlFor="onedrive_drive_id">{t("onedrive_drive_id")}</Label>
				<Input
					id="onedrive_drive_id"
					value={connectorStringValue(form, "drive_id")}
					onChange={(event) =>
						setConnectorValue("drive_id", event.target.value)
					}
					className={ADMIN_CONTROL_HEIGHT_CLASS}
					placeholder={t("onedrive_drive_id_placeholder")}
				/>
				<p className="text-xs leading-5 text-muted-foreground">
					{t("onedrive_drive_id_desc")}
				</p>
			</div>

			<div className="space-y-2">
				<Label htmlFor="onedrive_root_item_id">
					{t("onedrive_root_item_id")}
				</Label>
				<Input
					id="onedrive_root_item_id"
					value={connectorStringValue(form, "root_item_id", "root")}
					onChange={(event) =>
						setConnectorValue("root_item_id", event.target.value)
					}
					className={ADMIN_CONTROL_HEIGHT_CLASS}
					placeholder={t("onedrive_root_item_id_placeholder")}
				/>
				<p className="text-xs leading-5 text-muted-foreground">
					{t("onedrive_root_item_id_desc")}
				</p>
			</div>

			{accountMode === "sharepoint_site" ? (
				<div className="space-y-2">
					<Label htmlFor="onedrive_site_id">{t("onedrive_site_id")}</Label>
					<Input
						id="onedrive_site_id"
						value={connectorStringValue(form, "site_id")}
						onChange={(event) =>
							setConnectorValue("site_id", event.target.value)
						}
						className={ADMIN_CONTROL_HEIGHT_CLASS}
						placeholder={t("onedrive_site_id_placeholder")}
					/>
					<p className="text-xs leading-5 text-muted-foreground">
						{t("onedrive_site_id_desc")}
					</p>
				</div>
			) : accountMode === "group_drive" ? (
				<div className="space-y-2">
					<Label htmlFor="onedrive_group_id">{t("onedrive_group_id")}</Label>
					<Input
						id="onedrive_group_id"
						value={connectorStringValue(form, "group_id")}
						onChange={(event) =>
							setConnectorValue("group_id", event.target.value)
						}
						className={ADMIN_CONTROL_HEIGHT_CLASS}
						placeholder={t("onedrive_group_id_placeholder")}
					/>
					<p className="text-xs leading-5 text-muted-foreground">
						{t("onedrive_group_id_desc")}
					</p>
				</div>
			) : null}
		</>
	);
}
