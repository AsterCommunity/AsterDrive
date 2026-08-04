import { type UIEvent, useEffect, useState } from "react";
import { useTranslation } from "react-i18next";
import type {
	PolicyGroupFormData,
	PolicyGroupRuleForm,
} from "@/components/admin/policyGroupDialogShared";
import { Badge } from "@/components/ui/badge";
import { Button } from "@/components/ui/button";
import {
	Dialog,
	DialogContent,
	DialogDescription,
	DialogFooter,
	DialogHeader,
	DialogTitle,
} from "@/components/ui/dialog";
import { Icon } from "@/components/ui/icon";
import { Input } from "@/components/ui/input";
import { Label } from "@/components/ui/label";
import {
	Select,
	SelectContent,
	SelectGroup,
	SelectItem,
	SelectLabel,
	SelectSeparator,
	SelectTrigger,
	SelectValue,
} from "@/components/ui/select";
import { Switch } from "@/components/ui/switch";
import {
	ADMIN_CONTROL_HEIGHT_CLASS,
	ADMIN_ICON_BUTTON_CLASS,
} from "@/lib/constants";
import type { StoragePolicy } from "@/types/api";

export type PolicyLookup = Pick<StoragePolicy, "connector_id" | "id" | "name">;

interface PolicyGroupDialogProps {
	open: boolean;
	mode: "create" | "edit";
	form: PolicyGroupFormData;
	formError: string | null;
	hasMorePolicies: boolean;
	policies: PolicyLookup[];
	policiesLoading: boolean;
	policiesLoadingMore: boolean;
	policiesTotal: number;
	submitting: boolean;
	onAddRule: () => void;
	onFieldChange: <K extends keyof PolicyGroupFormData>(
		key: K,
		value: PolicyGroupFormData[K],
	) => void;
	onLoadMorePolicies: () => void | Promise<void>;
	onOpenChange: (open: boolean) => void;
	onRefreshPolicies: () => void | Promise<void>;
	onRemoveRule: (ruleKey: string) => void;
	onRuleFieldChange: <K extends Exclude<keyof PolicyGroupRuleForm, "key">>(
		ruleKey: string,
		key: K,
		value: PolicyGroupRuleForm[K],
	) => void;
	onSubmit: () => void;
}

function matchesPolicySearch(policy: PolicyLookup, query: string) {
	if (!query) return true;
	const normalizedQuery = query.toLowerCase();
	return (
		policy.name.toLowerCase().includes(normalizedQuery) ||
		String(policy.id).includes(normalizedQuery) ||
		policy.connector_id.toLowerCase().includes(normalizedQuery)
	);
}

function findPolicyName(policies: PolicyLookup[], policyId: string) {
	if (!policyId) return null;
	return (
		policies.find((policy) => String(policy.id) === policyId)?.name ??
		`#${policyId}`
	);
}

export function PolicyGroupDialog({
	open,
	mode,
	form,
	formError,
	hasMorePolicies,
	policies,
	policiesLoading,
	policiesLoadingMore,
	policiesTotal,
	submitting,
	onAddRule,
	onFieldChange,
	onLoadMorePolicies,
	onOpenChange,
	onRefreshPolicies,
	onRemoveRule,
	onRuleFieldChange,
	onSubmit,
}: PolicyGroupDialogProps) {
	const { t } = useTranslation("admin");
	const [policySearch, setPolicySearch] = useState("");
	const normalizedPolicySearch = policySearch.trim().toLowerCase();
	const filteredPolicies = policies.filter((policy) =>
		matchesPolicySearch(policy, normalizedPolicySearch),
	);

	useEffect(() => {
		if (!open) {
			setPolicySearch("");
			return;
		}

		if (
			normalizedPolicySearch &&
			filteredPolicies.length === 0 &&
			!policiesLoading &&
			!policiesLoadingMore &&
			hasMorePolicies
		) {
			void onLoadMorePolicies();
		}
	}, [
		filteredPolicies.length,
		hasMorePolicies,
		normalizedPolicySearch,
		onLoadMorePolicies,
		open,
		policiesLoading,
		policiesLoadingMore,
	]);

	const handleDialogOpenChange = (nextOpen: boolean) => {
		if (!nextOpen) {
			setPolicySearch("");
		}
		onOpenChange(nextOpen);
	};

	const handlePolicySelectOpenChange = (selectOpen: boolean) => {
		if (selectOpen && policies.length === 0 && !policiesLoading) {
			void onRefreshPolicies();
		}
	};

	const handlePolicySelectScroll = (event: UIEvent<HTMLDivElement>) => {
		if (policiesLoading || policiesLoadingMore || !hasMorePolicies) {
			return;
		}
		const target = event.currentTarget;
		if (target.scrollTop + target.clientHeight >= target.scrollHeight - 24) {
			void onLoadMorePolicies();
		}
	};

	const getSelectablePolicies = (selectedPolicyId: string) => {
		if (!selectedPolicyId) {
			return filteredPolicies;
		}

		const selectedPolicy = policies.find(
			(policy) => String(policy.id) === selectedPolicyId,
		);
		if (!selectedPolicy) {
			return filteredPolicies;
		}
		if (filteredPolicies.some((policy) => policy.id === selectedPolicy.id)) {
			return filteredPolicies;
		}
		return [selectedPolicy, ...filteredPolicies];
	};

	return (
		<Dialog open={open} onOpenChange={handleDialogOpenChange}>
			<DialogContent className="flex max-h-[min(90vh,calc(100vh-2rem))] flex-col gap-0 overflow-hidden p-0 sm:max-w-[calc(100%-2rem)] lg:max-w-5xl">
				<form
					className="flex min-h-0 flex-1 flex-col overflow-hidden"
					autoComplete="off"
					onSubmit={(event) => {
						event.preventDefault();
						onSubmit();
					}}
				>
					<DialogHeader className="shrink-0 border-b px-6 pt-5 pb-0 pr-14">
						<DialogTitle>
							{mode === "edit"
								? t("edit_policy_group")
								: t("create_policy_group")}
						</DialogTitle>
						<DialogDescription>
							{t("policy_group_dialog_desc")}
						</DialogDescription>
					</DialogHeader>
					<div className="min-h-0 flex-1 overflow-y-auto px-6 pt-6 pb-5">
						<div className="grid gap-6 lg:grid-cols-[320px_minmax(0,1fr)]">
							<section className="space-y-4 rounded-xl border bg-muted/20 p-4 lg:sticky lg:top-0 lg:self-start">
								<div className="space-y-1">
									<h3 className="text-sm font-semibold text-foreground">
										{t("policy_group_overview")}
									</h3>
									<p className="text-xs text-muted-foreground">
										{t("policy_group_overview_desc")}
									</p>
								</div>

								<div className="space-y-2">
									<Label htmlFor="policy-group-name">{t("core:name")}</Label>
									<Input
										id="policy-group-name"
										value={form.name}
										onChange={(event) =>
											onFieldChange("name", event.target.value)
										}
										className={ADMIN_CONTROL_HEIGHT_CLASS}
										aria-invalid={!form.name.trim() ? true : undefined}
										required
									/>
								</div>

								<div className="space-y-2">
									<Label htmlFor="policy-group-description">
										{t("policy_group_description")}
									</Label>
									<Input
										id="policy-group-description"
										value={form.description}
										onChange={(event) =>
											onFieldChange("description", event.target.value)
										}
										className={ADMIN_CONTROL_HEIGHT_CLASS}
										placeholder={t("policy_group_description_placeholder")}
									/>
								</div>

								<div className="space-y-3 rounded-xl border bg-background p-4">
									<div className="flex items-center justify-between gap-3">
										<div className="space-y-1">
											<p className="text-sm font-medium text-foreground">
												{t("policy_group_enabled")}
											</p>
											<p className="text-xs text-muted-foreground">
												{t("policy_group_enabled_desc")}
											</p>
										</div>
										<Switch
											id="policy-group-enabled"
											checked={form.isEnabled}
											onCheckedChange={(checked) =>
												onFieldChange("isEnabled", checked)
											}
										/>
									</div>
									<div className="flex items-center justify-between gap-3">
										<div className="space-y-1">
											<p className="text-sm font-medium text-foreground">
												{t("policy_group_default")}
											</p>
											<p className="text-xs text-muted-foreground">
												{t("policy_group_default_desc")}
											</p>
										</div>
										<Switch
											id="policy-group-default"
											checked={form.isDefault}
											onCheckedChange={(checked) =>
												onFieldChange("isDefault", checked)
											}
										/>
									</div>
								</div>

								<div className="rounded-xl border bg-background p-4">
									<p className="text-xs font-medium uppercase tracking-wide text-muted-foreground">
										{t("policy_group_summary")}
									</p>
									<div className="mt-3 flex flex-wrap gap-2">
										<Badge variant="outline">
											{t("policy_group_rules_count", {
												count: form.items.length,
											})}
										</Badge>
										{form.isDefault ? (
											<Badge className="border-blue-300 bg-blue-100 text-blue-700 dark:border-blue-700 dark:bg-blue-900 dark:text-blue-300">
												{t("is_default")}
											</Badge>
										) : null}
										<Badge
											variant="outline"
											className={
												form.isEnabled
													? "border-emerald-500/60 bg-emerald-500/10 text-emerald-600 dark:text-emerald-300"
													: "border-muted-foreground/30 bg-muted text-muted-foreground"
											}
										>
											{form.isEnabled
												? t("core:active")
												: t("core:disabled_status")}
										</Badge>
									</div>
								</div>
							</section>

							<section className="space-y-4 rounded-xl border bg-background p-4">
								<div className="space-y-3">
									<div className="flex items-start justify-between gap-4">
										<div className="space-y-1">
											<h3 className="text-sm font-semibold text-foreground">
												{t("policy_group_rules")}
											</h3>
											<p className="text-xs text-muted-foreground">
												{t("policy_group_rules_desc")}
											</p>
										</div>
										<Button
											type="button"
											variant="outline"
											size="sm"
											className={ADMIN_CONTROL_HEIGHT_CLASS}
											onClick={onAddRule}
											disabled={policies.length === 0}
										>
											<Icon name="Plus" className="mr-1 size-4" />
											{t("policy_group_add_rule")}
										</Button>
									</div>

									<div className="grid gap-3 md:grid-cols-[minmax(0,1fr)_auto] md:items-start">
										<div className="space-y-2">
											<Label htmlFor="policy-group-search">
												{t("policy_group_policy_search")}
											</Label>
											<Input
												id="policy-group-search"
												value={policySearch}
												onChange={(event) =>
													setPolicySearch(event.target.value)
												}
												className={ADMIN_CONTROL_HEIGHT_CLASS}
												placeholder={t(
													"policy_group_policy_search_placeholder",
												)}
											/>
										</div>
										<div className="rounded-lg border bg-muted/20 px-3 py-2 text-xs text-muted-foreground">
											{policiesLoadingMore
												? t("policy_group_loading_more_policies")
												: hasMorePolicies
													? t("policy_group_scroll_to_load_more")
													: t("policy_group_all_policies_loaded", {
															total: policiesTotal,
														})}
										</div>
									</div>
								</div>

								{policies.length === 0 ? (
									<div className="rounded-xl border border-dashed px-4 py-6 text-sm text-muted-foreground">
										{t("policy_group_no_policies_available")}
									</div>
								) : null}

								<div className="space-y-3">
									{form.items.map((item, index) => {
										const selectablePolicies = getSelectablePolicies(
											item.policyId,
										);
										const selectablePolicyOptions = selectablePolicies.map(
											(policy) => ({
												label: policy.name,
												value: String(policy.id),
											}),
										);
										const selectedPolicyName = findPolicyName(
											policies,
											item.policyId,
										);

										return (
											<div
												key={item.key}
												className="space-y-4 rounded-xl border bg-muted/20 p-4"
											>
												<div className="flex items-center justify-between gap-3">
													<div>
														<p className="text-sm font-medium text-foreground">
															{t("policy_group_rule_title", {
																index: index + 1,
															})}
														</p>
														<p className="text-xs text-muted-foreground">
															{t("policy_group_rule_hint")}
														</p>
													</div>
													<Button
														type="button"
														variant="ghost"
														size="icon"
														className={`${ADMIN_ICON_BUTTON_CLASS} text-muted-foreground`}
														onClick={() => onRemoveRule(item.key)}
														disabled={form.items.length === 1}
														aria-label={t("policy_group_remove_rule")}
													>
														<Icon name="Trash" className="size-3.5" />
													</Button>
												</div>

												<div className="grid gap-4 md:grid-cols-[minmax(0,1.5fr)_120px]">
													<div className="space-y-2">
														<Label>{t("assign_policy")}</Label>
														<Select
															items={selectablePolicyOptions}
															value={item.policyId}
															onOpenChange={handlePolicySelectOpenChange}
															onValueChange={(value) =>
																onRuleFieldChange(
																	item.key,
																	"policyId",
																	value ?? "",
																)
															}
														>
															<SelectTrigger
																className={`${ADMIN_CONTROL_HEIGHT_CLASS} w-full`}
															>
																<SelectValue placeholder={t("select_policy")}>
																	{selectedPolicyName}
																</SelectValue>
															</SelectTrigger>
															<SelectContent
																className="max-h-64"
																onScroll={handlePolicySelectScroll}
															>
																{selectablePolicies.map((policy) => (
																	<SelectItem
																		key={policy.id}
																		value={String(policy.id)}
																	>
																		{policy.name}
																	</SelectItem>
																))}
																{selectablePolicies.length === 0 ? (
																	<SelectGroup>
																		<SelectLabel>
																			{t("policy_group_no_filtered_policies")}
																		</SelectLabel>
																	</SelectGroup>
																) : null}
																{policiesLoadingMore || hasMorePolicies ? (
																	<>
																		{selectablePolicies.length > 0 ? (
																			<SelectSeparator />
																		) : null}
																		<SelectGroup>
																			<SelectLabel>
																				{policiesLoadingMore
																					? t(
																							"policy_group_loading_more_policies",
																						)
																					: t(
																							"policy_group_scroll_to_load_more",
																						)}
																			</SelectLabel>
																		</SelectGroup>
																	</>
																) : null}
															</SelectContent>
														</Select>
													</div>

													<div className="space-y-2">
														<Label htmlFor={`${item.key}-priority`}>
															{t("policy_group_priority")}
														</Label>
														<Input
															id={`${item.key}-priority`}
															type="number"
															min="1"
															step="1"
															value={item.priority}
															onChange={(event) =>
																onRuleFieldChange(
																	item.key,
																	"priority",
																	event.target.value,
																)
															}
															className={ADMIN_CONTROL_HEIGHT_CLASS}
														/>
													</div>
												</div>

												<div className="grid gap-4 md:grid-cols-2">
													<div className="space-y-2">
														<Label htmlFor={`${item.key}-min-size`}>
															{t("policy_group_min_size_mb")}
														</Label>
														<Input
															id={`${item.key}-min-size`}
															type="number"
															min="0"
															step="any"
															value={item.minFileSizeMb}
															onChange={(event) =>
																onRuleFieldChange(
																	item.key,
																	"minFileSizeMb",
																	event.target.value,
																)
															}
															placeholder={t("policy_group_size_unlimited")}
															className={ADMIN_CONTROL_HEIGHT_CLASS}
														/>
													</div>
													<div className="space-y-2">
														<Label htmlFor={`${item.key}-max-size`}>
															{t("policy_group_max_size_mb")}
														</Label>
														<Input
															id={`${item.key}-max-size`}
															type="number"
															min="0"
															step="any"
															value={item.maxFileSizeMb}
															onChange={(event) =>
																onRuleFieldChange(
																	item.key,
																	"maxFileSizeMb",
																	event.target.value,
																)
															}
															placeholder={t("policy_group_size_unlimited")}
															className={ADMIN_CONTROL_HEIGHT_CLASS}
														/>
													</div>
												</div>
											</div>
										);
									})}
								</div>
							</section>
						</div>

						{formError ? (
							<div className="mt-4 rounded-xl border border-destructive/30 bg-destructive/5 px-4 py-3 text-sm text-destructive">
								{formError}
							</div>
						) : null}
					</div>
					<DialogFooter className="mx-0 mb-0 w-full shrink-0 flex-row items-center gap-2 rounded-b-xl px-6 py-3">
						<Button
							type="button"
							variant="outline"
							onClick={() => handleDialogOpenChange(false)}
						>
							{t("core:cancel")}
						</Button>
						<Button type="submit" disabled={submitting || policiesLoading}>
							{submitting ? (
								<Icon name="Spinner" className="mr-1 size-4 animate-spin" />
							) : (
								<Icon name="FloppyDisk" className="mr-1 size-4" />
							)}
							{mode === "edit" ? t("save_changes") : t("core:create")}
						</Button>
					</DialogFooter>
				</form>
			</DialogContent>
		</Dialog>
	);
}
