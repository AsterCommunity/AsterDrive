import { useAdminSettingsCategoryContent } from "@/components/admin/settings/AdminSettingsCategoryContentContext";
import { AdminSettingsCategoryHeader } from "@/components/admin/settings/AdminSettingsCategoryHeader";
import {
	CustomConfigRow,
	NewCustomRow,
} from "@/components/admin/settings/AdminSettingsConfigRows";
import { Button } from "@/components/ui/button";
import { Icon } from "@/components/ui/icon";

export function AdminSettingsCustomCategorySection({
	category,
	panelAnimationClass,
	showCategoryHeader,
}: {
	category: string;
	panelAnimationClass: string;
	showCategoryHeader: boolean;
}) {
	const {
		activeTab,
		addCustomDraftRow,
		deletedCustomConfigs,
		newCustomRows,
		restoreDeletedCustom,
		t,
		tabDirection,
		visibleCustomConfigs,
	} = useAdminSettingsCategoryContent();

	const customCategoryActions = (
		<div className="space-y-3">
			<p className="text-xs text-muted-foreground">
				{t("config_namespace_hint")}
			</p>
			<Button
				variant="ghost"
				size="sm"
				className="justify-start px-0"
				onClick={addCustomDraftRow}
			>
				<Icon name="Plus" className="size-4" />
				{t("add_custom_config")}
			</Button>
		</div>
	);

	return (
		<div
			key={`${activeTab}-${tabDirection}`}
			className={`space-y-6 ${panelAnimationClass}`}
		>
			{showCategoryHeader ? (
				<AdminSettingsCategoryHeader
					category={category}
					description={undefined}
					extra={customCategoryActions}
				/>
			) : (
				customCategoryActions
			)}

			{visibleCustomConfigs.length === 0 &&
			newCustomRows.length === 0 &&
			deletedCustomConfigs.length === 0 ? (
				<p className="text-sm text-muted-foreground">
					{t("custom_config_empty")}
				</p>
			) : null}

			{visibleCustomConfigs.length > 0 ? (
				<div className="max-w-4xl divide-y divide-border/40">
					{visibleCustomConfigs.map((config) => (
						<div key={config.key} className="py-4 first:pt-0 last:pb-0">
							<CustomConfigRow config={config} />
						</div>
					))}
				</div>
			) : null}

			{newCustomRows.length > 0 ? (
				<div className="max-w-4xl divide-y divide-border/40">
					{newCustomRows.map((row) => (
						<div key={row.id} className="py-4 first:pt-0 last:pb-0">
							<NewCustomRow row={row} />
						</div>
					))}
				</div>
			) : null}

			{deletedCustomConfigs.length > 0 ? (
				<div className="max-w-4xl space-y-4">
					<p className="text-sm text-muted-foreground">
						{t("custom_config_delete_staged")}
					</p>
					<div className="divide-y divide-border/40">
						{deletedCustomConfigs.map((config) => (
							<div
								key={config.key}
								className="space-y-2 py-4 opacity-70 first:pt-0 last:pb-0"
							>
								<p className="break-all font-mono text-sm line-through">
									{config.key}
								</p>
								<Button
									variant="ghost"
									size="sm"
									className="justify-start px-0"
									onClick={() => restoreDeletedCustom(config.key)}
								>
									{t("restore")}
								</Button>
							</div>
						))}
					</div>
				</div>
			) : null}
		</div>
	);
}
