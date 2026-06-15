import { useState } from "react";
import { AnimatedCollapsible } from "@/components/common/AnimatedCollapsible";
import { Button } from "@/components/ui/button";
import { Icon } from "@/components/ui/icon";
import { Label } from "@/components/ui/label";
import {
	Select,
	SelectContent,
	SelectItem,
	SelectTrigger,
	SelectValue,
} from "@/components/ui/select";
import { ADMIN_CONTROL_HEIGHT_CLASS } from "@/lib/constants";
import { cn } from "@/lib/utils";
import type { MicrosoftGraphCloud, OneDriveAccountMode } from "@/types/api";
import { OneDriveApplicationFields } from "./OneDriveApplicationFields";
import { OneDriveTargetFields } from "./OneDriveTargetFields";
import type { SharedFieldProps, Translate } from "./StoragePolicyFieldTypes";

function getCloudOptions(t: Translate) {
	return [
		{ label: t("onedrive_cloud_global"), value: "global" },
		{ label: t("onedrive_cloud_china"), value: "china" },
	] satisfies ReadonlyArray<{
		label: string;
		value: MicrosoftGraphCloud;
	}>;
}

function getAccountModeOptions(t: Translate, cloud: MicrosoftGraphCloud) {
	const accountModeOptions: Array<{
		label: string;
		value: OneDriveAccountMode;
	}> = [
		{
			label: t("onedrive_account_mode_work_or_school"),
			value: "work_or_school",
		},
		{
			label: t("onedrive_account_mode_sharepoint_site"),
			value: "sharepoint_site",
		},
		{ label: t("onedrive_account_mode_group_drive"), value: "group_drive" },
	];
	if (cloud !== "china") {
		accountModeOptions.splice(1, 0, {
			label: t("onedrive_account_mode_personal"),
			value: "personal",
		});
	}
	return accountModeOptions;
}

function OneDriveSetupNotice({ t }: { t: Translate }) {
	return (
		<div className="flex gap-2 rounded-lg border border-sky-500/25 bg-sky-500/5 p-3 text-xs leading-5 text-muted-foreground">
			<Icon name="Info" className="mt-0.5 size-4 shrink-0 text-sky-600" />
			<div className="space-y-1">
				<p className="font-medium text-foreground">
					{t("onedrive_setup_notice_title")}
				</p>
				<ul className="space-y-1">
					<li>{t("onedrive_setup_notice_redirect_uri")}</li>
					<li>{t("onedrive_setup_notice_permissions")}</li>
					<li>{t("onedrive_setup_notice_cloud")}</li>
					<li>{t("onedrive_setup_notice_personal_china")}</li>
				</ul>
			</div>
		</div>
	);
}

export function OneDriveConnectionFields({
	clientIdError,
	form,
	mode = "edit",
	onFieldChange,
	showCreateValidation = false,
	t,
}: SharedFieldProps & {
	clientIdError?: string | null;
	mode?: "create" | "edit";
	showCreateValidation?: boolean;
}) {
	const [advancedOpen, setAdvancedOpen] = useState(false);
	const cloudOptions = getCloudOptions(t);
	const accountModeOptions = getAccountModeOptions(t, form.onedrive_cloud);

	return (
		<div className="space-y-4">
			<div className="grid max-w-xl gap-4">
				<div className="space-y-2">
					<Label htmlFor="onedrive_cloud">{t("onedrive_cloud")}</Label>
					<Select
						items={cloudOptions}
						value={form.onedrive_cloud}
						onValueChange={(value) => {
							const nextCloud = (value ?? "global") as MicrosoftGraphCloud;
							onFieldChange("onedrive_cloud", nextCloud);
							onFieldChange(
								"onedrive_tenant",
								nextCloud === "china" ? "organizations" : "common",
							);
							if (
								nextCloud === "china" &&
								form.onedrive_account_mode === "personal"
							) {
								onFieldChange("onedrive_account_mode", "work_or_school");
								onFieldChange("onedrive_tenant", "organizations");
							}
						}}
					>
						<SelectTrigger id="onedrive_cloud">
							<SelectValue />
						</SelectTrigger>
						<SelectContent>
							{cloudOptions.map((option) => (
								<SelectItem key={option.value} value={option.value}>
									{option.label}
								</SelectItem>
							))}
						</SelectContent>
					</Select>
					<p className="text-xs leading-5 text-muted-foreground">
						{t("onedrive_cloud_desc")}
					</p>
				</div>
				<OneDriveSetupNotice t={t} />
				{mode === "create" ? (
					<OneDriveApplicationFields
						clientIdError={clientIdError}
						form={form}
						showValidation={showCreateValidation}
						t={t}
						onFieldChange={onFieldChange}
					/>
				) : null}
			</div>
			{mode === "edit" ? (
				<div className="space-y-3">
					<Button
						type="button"
						variant="outline"
						className={cn(ADMIN_CONTROL_HEIGHT_CLASS, "w-fit")}
						onClick={() => setAdvancedOpen((open) => !open)}
					>
						<Icon name="Gear" className="mr-1 size-3.5" />
						{t("onedrive_advanced_target")}
						<Icon
							name={advancedOpen ? "CaretUp" : "CaretDown"}
							className="ml-1 size-3.5"
						/>
					</Button>
					<AnimatedCollapsible open={advancedOpen}>
						<div className="grid gap-4 rounded-lg border border-border/70 bg-muted/20 p-4 md:grid-cols-2">
							<OneDriveTargetFields
								accountModeOptions={accountModeOptions}
								form={form}
								onFieldChange={onFieldChange}
								t={t}
							/>
						</div>
					</AnimatedCollapsible>
				</div>
			) : null}
		</div>
	);
}

export { OneDriveCredentialPanel } from "./OneDriveCredentialPanel";
