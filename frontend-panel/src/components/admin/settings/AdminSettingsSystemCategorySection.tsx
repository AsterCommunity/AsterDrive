import { useAdminSettingsCategoryContent } from "@/components/admin/settings/AdminSettingsCategoryContentContext";
import { AdminSettingsCategoryHeader } from "@/components/admin/settings/AdminSettingsCategoryHeader";
import { SystemConfigRow } from "@/components/admin/settings/AdminSettingsConfigRows";
import { ADMIN_SETTINGS_CONTENT_MAX_WIDTH_CLASS } from "@/components/admin/settings/adminSettingsAnimation";
import {
	AnimatedCollapsible,
	getConfigValueType,
	getMailTemplateFieldOrder,
	getMailTemplateGroupId,
	getMailTemplateGroupOrderIndex,
	getSubcategoryGroupKey,
	isMultilineType,
} from "@/components/admin/settings/adminSettingsContentShared";
import { Button } from "@/components/ui/button";
import { Icon } from "@/components/ui/icon";
import { cn } from "@/lib/utils";
import type { SystemConfig } from "@/types/api";

const DEFAULT_COLLAPSED_SUBCATEGORY_GROUP_KEYS = new Set([
	"file_processing:archive_extract",
	"file_processing:archive_preview",
	"file_processing:offline_download",
]);

function AdminSettingsMailTemplateGroup({
	changedCount,
	configs,
	groupKey,
	templateExpanded,
	templateGroupId,
}: {
	changedCount: number;
	configs: SystemConfig[];
	groupKey: string;
	templateExpanded: boolean;
	templateGroupId: string;
}) {
	const { getMailTemplateGroupLabel, t, toggleTemplateGroup } =
		useAdminSettingsCategoryContent();

	return (
		<section
			key={groupKey}
			className="overflow-hidden rounded-lg border border-border/60 bg-background"
		>
			<Button
				variant="ghost"
				size="sm"
				className="flex h-auto w-full items-center justify-between gap-3 rounded-none px-3 py-2.5 text-left"
				aria-expanded={templateExpanded}
				onClick={() => toggleTemplateGroup(groupKey, !templateExpanded)}
			>
				<span className="min-w-0 space-y-1">
					<span className="block text-sm font-medium">
						{getMailTemplateGroupLabel(templateGroupId)}
					</span>
					{changedCount > 0 ? (
						<span className="block text-xs text-primary">
							{t("settings_save_notice", {
								count: changedCount,
							})}
						</span>
					) : null}
				</span>
				<span className="flex shrink-0 items-center gap-2 text-xs text-muted-foreground">
					<span>
						{templateExpanded
							? t("settings_section_collapse")
							: t("settings_section_expand")}
					</span>
					<Icon
						name="CaretDown"
						className={cn(
							"size-4 transition-transform",
							templateExpanded ? "rotate-180" : "",
						)}
					/>
				</span>
			</Button>
			<AnimatedCollapsible
				open={templateExpanded}
				contentClassName={cn(
					"px-3 transition-colors duration-[180ms] ease-out motion-reduce:transition-none",
					templateExpanded
						? "border-t border-border/40"
						: "border-t border-transparent",
				)}
			>
				<div className="divide-y divide-border/40">
					{configs.map((config) => (
						<div key={config.key} className="py-4 first:pt-4 last:pb-4">
							<SystemConfigRow config={config} />
						</div>
					))}
				</div>
			</AnimatedCollapsible>
		</section>
	);
}

function AdminSettingsSystemSubcategoryCard({
	category,
	group,
}: {
	category: string;
	group: {
		category: string;
		subcategory?: string;
		configs: SystemConfig[];
	};
}) {
	const {
		expandedSubcategoryGroups,
		expandedTemplateGroups,
		getDraftValue,
		getSubcategoryDescription,
		getSubcategoryLabel,
		openTestEmailDialog,
		t,
		toggleSubcategoryGroup,
	} = useAdminSettingsCategoryContent();
	const groupKey = getSubcategoryGroupKey(category, group.subcategory);
	const isMailTemplateSection =
		category === "mail" && group.subcategory === "template";
	const defaultCollapsed =
		DEFAULT_COLLAPSED_SUBCATEGORY_GROUP_KEYS.has(groupKey) ||
		(category === "runtime" && group.subcategory !== "background_task");
	const collapsible =
		!isMailTemplateSection &&
		(defaultCollapsed ||
			group.configs.some((config) =>
				isMultilineType(getConfigValueType(config)),
			));
	const defaultExpanded = !collapsible;
	const expanded = expandedSubcategoryGroups[groupKey] ?? defaultExpanded;
	const groupDescription = getSubcategoryDescription(
		category,
		group.subcategory,
	);
	const extra =
		category === "mail" && group.subcategory === "config" ? (
			<div className="flex flex-col items-start gap-2 lg:items-end">
				<Button variant="outline" size="sm" onClick={openTestEmailDialog}>
					<Icon name="EnvelopeSimple" className="size-4" />
					{t("mail_send_test_email")}
				</Button>
				<p className="max-w-xs text-xs text-muted-foreground lg:text-right">
					{t("mail_send_test_email_hint")}
				</p>
			</div>
		) : null;
	const mailTemplateGroups = isMailTemplateSection
		? Array.from(
				group.configs.reduce((map, config) => {
					const templateGroupId = getMailTemplateGroupId(config.key);
					const existingGroup = map.get(templateGroupId);
					if (existingGroup) {
						existingGroup.push(config);
						return map;
					}

					map.set(templateGroupId, [config]);
					return map;
				}, new Map<string, SystemConfig[]>()),
			)
				.toSorted(([left], [right]) => {
					return (
						getMailTemplateGroupOrderIndex(left) -
							getMailTemplateGroupOrderIndex(right) || left.localeCompare(right)
					);
				})
				.map(([templateGroupId, configs]) => ({
					configs: configs.toSorted(
						(left, right) =>
							getMailTemplateFieldOrder(left.key) -
								getMailTemplateFieldOrder(right.key) ||
							left.key.localeCompare(right.key),
					),
					groupKey: `${groupKey}:${templateGroupId}`,
					templateGroupId,
				}))
		: [];

	return (
		<section
			key={groupKey}
			className="overflow-hidden rounded-lg border border-border/60 bg-background"
		>
			<div className="flex flex-col gap-3 px-4 py-3 lg:flex-row lg:items-start lg:justify-between">
				<div className="min-w-0 flex-1 space-y-1">
					<h4 className="text-base font-semibold tracking-tight">
						{getSubcategoryLabel(category, group.subcategory)}
					</h4>
					{groupDescription ? (
						<p className="max-w-3xl break-words text-sm leading-5 text-muted-foreground">
							{groupDescription}
						</p>
					) : null}
				</div>
				<div className="flex flex-col items-start gap-3 lg:items-end">
					{extra}
					{collapsible ? (
						<Button
							variant="ghost"
							size="sm"
							className="justify-start px-0 lg:px-3"
							aria-expanded={expanded}
							onClick={() => toggleSubcategoryGroup(groupKey, !expanded)}
						>
							{expanded
								? t("settings_section_collapse")
								: t("settings_section_expand")}
							<Icon
								name="CaretDown"
								className={cn(
									"size-4 transition-transform",
									expanded ? "rotate-180" : "",
								)}
							/>
						</Button>
					) : null}
				</div>
			</div>
			{!collapsible || expanded ? (
				<div className="border-t border-border/40 px-4">
					{isMailTemplateSection ? (
						<div className="space-y-3 py-4">
							{mailTemplateGroups.map((templateGroup) => (
								<AdminSettingsMailTemplateGroup
									key={templateGroup.groupKey}
									changedCount={
										templateGroup.configs.filter(
											(config) => getDraftValue(config) !== config.value,
										).length
									}
									configs={templateGroup.configs}
									groupKey={templateGroup.groupKey}
									templateExpanded={
										expandedTemplateGroups[templateGroup.groupKey] ?? false
									}
									templateGroupId={templateGroup.templateGroupId}
								/>
							))}
						</div>
					) : (
						<div className="divide-y divide-border/40">
							{group.configs.map((config) => (
								<div key={config.key} className="py-4 first:pt-4 last:pb-4">
									<SystemConfigRow config={config} />
								</div>
							))}
						</div>
					)}
				</div>
			) : null}
		</section>
	);
}

export function AdminSettingsSystemCategorySection({
	category,
	panelAnimationClass,
	showCategoryHeader,
}: {
	category: string;
	panelAnimationClass: string;
	showCategoryHeader: boolean;
}) {
	const { activeTab, systemGroups, systemSubcategoryGroups, tabDirection } =
		useAdminSettingsCategoryContent();
	const systemConfigGroups = systemSubcategoryGroups[category] ?? [];
	const hasSubcategorySections =
		systemConfigGroups.length > 1 ||
		systemConfigGroups.some((group) => group.subcategory);

	return (
		<div
			key={`${activeTab}-${tabDirection}`}
			className={`space-y-6 ${panelAnimationClass}`}
		>
			{showCategoryHeader ? (
				<AdminSettingsCategoryHeader
					category={category}
					description={undefined}
				/>
			) : null}
			{!hasSubcategorySections ? (
				<div
					className={cn(
						ADMIN_SETTINGS_CONTENT_MAX_WIDTH_CLASS,
						"divide-y divide-border/40",
					)}
				>
					{(systemGroups[category] ?? []).map((config) => (
						<div key={config.key} className="py-4 first:pt-0 last:pb-0">
							<SystemConfigRow config={config} />
						</div>
					))}
				</div>
			) : (
				<div
					className={cn(ADMIN_SETTINGS_CONTENT_MAX_WIDTH_CLASS, "space-y-3")}
				>
					{systemConfigGroups.map((group) => (
						<AdminSettingsSystemSubcategoryCard
							key={getSubcategoryGroupKey(category, group.subcategory)}
							category={category}
							group={group}
						/>
					))}
				</div>
			)}
		</div>
	);
}
