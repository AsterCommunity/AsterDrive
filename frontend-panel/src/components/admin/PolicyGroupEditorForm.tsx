import { type UIEvent, useEffect, useLayoutEffect, useRef, useState } from "react";
import { useTranslation } from "react-i18next";
import type {
	PolicyGroupFormData,
	PolicyGroupRuleForm,
	PolicyGroupRuleTargetForm,
} from "@/components/admin/policyGroupEditorShared";
import {
	buildPolicyGroupRuleTargetForm,
	bytesToMbInput,
	MAX_POLICY_GROUP_RULE_NAME_LENGTH,
	mbInputToBytes,
} from "@/components/admin/policyGroupEditorShared";
import { Badge } from "@/components/ui/badge";
import { Button } from "@/components/ui/button";
import {
	Dialog,
	DialogContent,
	DialogDescription,
	DialogHeader,
	DialogTitle,
} from "@/components/ui/dialog";
import { Icon } from "@/components/ui/icon";
import { Input } from "@/components/ui/input";
import { ItemCheckbox } from "@/components/ui/item-checkbox";
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
import { cn } from "@/lib/utils";
import type { components } from "@/services/api.generated";
import type { StoragePolicy } from "@/types/api";

export type PolicyLookup = Pick<StoragePolicy, "connector_id" | "id" | "name">;

type FileCategory = components["schemas"]["FileCategory"];

const FILE_CATEGORY_KEYS: readonly FileCategory[] = [
	"image",
	"video",
	"audio",
	"document",
	"spreadsheet",
	"presentation",
	"archive",
	"code",
	"other",
];

interface PolicyGroupEditorFormProps {
	mode: "create" | "edit";
	form: PolicyGroupFormData;
	formError: string | null;
	hasMorePolicies: boolean;
	policies: PolicyLookup[];
	policiesLoading: boolean;
	policiesLoadingMore: boolean;
	onAddRule: () => void;
	onFieldChange: <K extends keyof PolicyGroupFormData>(
		key: K,
		value: PolicyGroupFormData[K],
	) => void;
	onLoadMorePolicies: () => void | Promise<void>;
	onMoveRule: (ruleKey: string, direction: -1 | 1) => void;
	onOpenSimulation?: () => void;
	onRefreshPolicies: () => void | Promise<void>;
	onRemoveRule: (ruleKey: string) => void;
	onReorderRule: (ruleKey: string, targetIndex: number) => void;
	onRuleFieldChange: <K extends Exclude<keyof PolicyGroupRuleForm, "key">>(
		ruleKey: string,
		key: K,
		value: PolicyGroupRuleForm[K],
	) => void;
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

function findPolicy(policies: PolicyLookup[], policyId: string) {
	if (!policyId) return null;
	return policies.find((candidate) => String(candidate.id) === policyId) ?? null;
}

function formatPolicyTarget(policy: PolicyLookup) {
	return (
		<>
			<span className="truncate">{policy.name}</span>
			<span className="shrink-0 text-xs text-muted-foreground">
				#{policy.id} · {policy.connector_id}
			</span>
		</>
	);
}

function formatPolicyTargetLabel(policy: PolicyLookup) {
	return `${policy.name} (#${policy.id} · ${policy.connector_id})`;
}

interface CategoryCheckboxGroupProps {
	hint: string;
	legend: string;
	t: ReturnType<typeof useTranslation>["t"];
	value: FileCategory[];
	onChange: (next: FileCategory[]) => void;
}

function CategoryCheckboxGroup({
	hint,
	legend,
	t,
	value,
	onChange,
}: CategoryCheckboxGroupProps) {
	const toggle = (category: FileCategory) => {
		onChange(
			value.includes(category)
				? value.filter((item) => item !== category)
				: [...value, category],
		);
	};
	const allSelected = value.length === FILE_CATEGORY_KEYS.length;

	return (
		<fieldset className="space-y-2">
			<legend className="w-full">
				<span className="flex items-center justify-between gap-2">
					<span className="text-sm font-medium">{legend}</span>
					<button
						type="button"
						className="text-xs text-muted-foreground hover:text-foreground"
						onClick={() => onChange(allSelected ? [] : [...FILE_CATEGORY_KEYS])}
					>
						{allSelected
							? t("policy_group_category_deselect_all")
							: t("policy_group_category_select_all")}
					</button>
				</span>
			</legend>
			<div className="flex flex-wrap gap-x-4 gap-y-2.5">
				{FILE_CATEGORY_KEYS.map((category) => {
					const checked = value.includes(category);
					return (
						<span
							key={category}
							className="flex items-center gap-2 text-xs text-muted-foreground"
						>
							<ItemCheckbox
								checked={checked}
								onChange={() => toggle(category)}
							/>
							<button
								type="button"
								className="cursor-pointer select-none hover:text-foreground"
								onClick={() => toggle(category)}
								aria-label={t(`policy_group_category_${category}`)}
							>
								<span className="text-foreground">
									{t(`policy_group_category_${category}`)}
								</span>
								<code className="ml-1">{category}</code>
							</button>
						</span>
					);
				})}
			</div>
			<p className="text-xs text-muted-foreground">{hint}</p>
		</fieldset>
	);
}

function DragHandleIcon({ className }: { className?: string }) {
	return (
		<svg
			width="10"
			height="16"
			viewBox="0 0 10 16"
			fill="currentColor"
			className={className}
			aria-hidden="true"
		>
			<circle cx="2.5" cy="2.5" r="1.5" />
			<circle cx="7.5" cy="2.5" r="1.5" />
			<circle cx="2.5" cy="8" r="1.5" />
			<circle cx="7.5" cy="8" r="1.5" />
			<circle cx="2.5" cy="13.5" r="1.5" />
			<circle cx="7.5" cy="13.5" r="1.5" />
		</svg>
	);
}

interface RuleCardProps {
	index: number;
	isDragging: boolean;
	isNew: boolean;
	item: PolicyGroupRuleForm;
	policies: PolicyLookup[];
	filteredPolicies: PolicyLookup[];
	hasMorePolicies: boolean;
	policiesLoading: boolean;
	policiesLoadingMore: boolean;
	ruleCount: number;
	t: ReturnType<typeof useTranslation>["t"];
	onDragEnd: () => void;
	onDragOverCard: (hoverIndex: number, after: boolean) => void;
	onDragStart: (ruleKey: string) => void;
	onLoadMorePolicies: () => void | Promise<void>;
	onMoveRule: (ruleKey: string, direction: -1 | 1) => void;
	onRefreshPolicies: () => void | Promise<void>;
	onRemoveRule: (ruleKey: string) => void;
	onRuleFieldChange: PolicyGroupEditorFormProps["onRuleFieldChange"];
}

function RuleCard({
	index,
	isDragging,
	isNew,
	item,
	policies,
	filteredPolicies,
	hasMorePolicies,
	policiesLoading,
	policiesLoadingMore,
	ruleCount,
	t,
	onDragEnd,
	onDragOverCard,
	onDragStart,
	onLoadMorePolicies,
	onMoveRule,
	onRefreshPolicies,
	onRemoveRule,
	onRuleFieldChange,
}: RuleCardProps) {
	const [confirmingDelete, setConfirmingDelete] = useState(false);
	const [leaving, setLeaving] = useState(false);
	// 新增卡片从 0 高度展开，把下方内容平滑推开而不是瞬间撑开
	const [expanded, setExpanded] = useState(!isNew);
	const deleteConfirmTimer = useRef<ReturnType<typeof setTimeout> | null>(null);
	const removeTimer = useRef<ReturnType<typeof setTimeout> | null>(null);

	useEffect(() => {
		if (expanded) return;
		const frame = requestAnimationFrame(() => setExpanded(true));
		return () => cancelAnimationFrame(frame);
	}, [expanded]);

	useEffect(() => {
		return () => {
			if (deleteConfirmTimer.current) {
				clearTimeout(deleteConfirmTimer.current);
			}
			if (removeTimer.current) {
				clearTimeout(removeTimer.current);
			}
		};
	}, []);

	// 删除规则：第一次点击进入确认态并显示文案，再按一次先播放高度
	// 收缩动画再真正移除，3 秒无操作自动复位——不打断工作流。
	const handleRemoveClick = () => {
		if (confirmingDelete) {
			if (deleteConfirmTimer.current) {
				clearTimeout(deleteConfirmTimer.current);
			}
			setConfirmingDelete(false);
			setLeaving(true);
			removeTimer.current = setTimeout(() => {
				onRemoveRule(item.key);
			}, 220);
			return;
		}
		setConfirmingDelete(true);
		deleteConfirmTimer.current = setTimeout(() => {
			setConfirmingDelete(false);
		}, 3000);
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

	const setTarget = <K extends Exclude<keyof PolicyGroupRuleTargetForm, "key">>(
		targetKey: string,
		key: K,
		value: PolicyGroupRuleTargetForm[K],
	) => {
		onRuleFieldChange(
			item.key,
			"targets",
			item.targets.map((target) =>
				target.key === targetKey ? { ...target, [key]: value } : target,
			),
		);
	};

	const removeTarget = (targetKey: string) => {
		onRuleFieldChange(
			item.key,
			"targets",
			item.targets.filter((target) => target.key !== targetKey),
		);
	};

	const addTarget = () => {
		const usedPolicyIds = new Set(
			item.targets.map((target) => target.policyId),
		);
		const nextPolicy = policies.find(
			(policy) => !usedPolicyIds.has(String(policy.id)),
		);
		onRuleFieldChange(item.key, "targets", [
			...item.targets,
			buildPolicyGroupRuleTargetForm(nextPolicy?.id ?? null),
		]);
	};

	return (
		<div
			className={cn(
				"grid transition-[grid-template-rows,opacity] duration-200 motion-reduce:transition-none",
				expanded && !leaving ? "grid-rows-[1fr]" : "grid-rows-[0fr] opacity-0",
			)}
			data-rule-key={item.key}
		>
			{/* biome-ignore lint/a11y/noStaticElementInteractions: drag reorder container; keyboard/touch users get the move buttons */}
			<div
				className={cn(
					"min-h-0 overflow-hidden space-y-4 rounded-xl bg-muted/30 p-4 transition-opacity",
					isDragging && "opacity-40",
					leaving && "pointer-events-none",
				)}
				onDragOver={(event) => {
					event.preventDefault();
					if (isDragging) return;
					const rect = event.currentTarget.getBoundingClientRect();
					const after = event.clientY - rect.top > rect.height / 2;
					onDragOverCard(index, after);
				}}
				onDrop={(event) => {
					event.preventDefault();
					onDragEnd();
				}}
			>
				<div className="flex items-center justify-between gap-3">
					<div className="flex min-w-0 items-center gap-2">
						<button
							type="button"
							className="cursor-grab touch-none rounded-md px-1 py-0.5 text-muted-foreground/70 hover:bg-muted hover:text-foreground active:cursor-grabbing"
							title={t("policy_group_rule_drag_handle")}
							aria-label={t("policy_group_rule_drag_handle")}
							draggable
							onDragStart={(event) => {
								event.dataTransfer.effectAllowed = "move";
								onDragStart(item.key);
							}}
							onDragEnd={onDragEnd}
						>
							<DragHandleIcon />
						</button>
						<Input
							value={item.name}
							maxLength={MAX_POLICY_GROUP_RULE_NAME_LENGTH}
							className={`${ADMIN_CONTROL_HEIGHT_CLASS} h-8! w-44 sm:w-56`}
							aria-label={t("policy_group_rule_name")}
							onChange={(event) =>
								onRuleFieldChange(item.key, "name", event.target.value)
							}
						/>
						<Badge variant="outline" className="text-muted-foreground">
							{t("policy_group_rule_order", { index: index + 1 })}
						</Badge>
						<span className="inline-flex gap-1 lg:hidden">
							<Button
								type="button"
								variant="outline"
								size="icon"
								className="size-7"
								onClick={() => onMoveRule(item.key, -1)}
								disabled={index === 0}
								aria-label={t("policy_group_rule_move_up")}
								title={t("policy_group_rule_move_up")}
							>
								<Icon name="ArrowUp" className="size-3.5" />
							</Button>
							<Button
								type="button"
								variant="outline"
								size="icon"
								className="size-7"
								onClick={() => onMoveRule(item.key, 1)}
								disabled={index === ruleCount - 1}
								aria-label={t("policy_group_rule_move_down")}
								title={t("policy_group_rule_move_down")}
							>
								<Icon name="ArrowDown" className="size-3.5" />
							</Button>
						</span>
					</div>
					{confirmingDelete ? (
						<Button
							type="button"
							variant="ghost"
							size="sm"
							className="h-8 px-2 text-destructive hover:bg-destructive/10 hover:text-destructive"
							onClick={handleRemoveClick}
						>
							{t("policy_group_remove_rule_confirm")}
						</Button>
					) : (
						<Button
							type="button"
							variant="ghost"
							size="icon"
							className={`${ADMIN_ICON_BUTTON_CLASS} text-muted-foreground`}
							onClick={handleRemoveClick}
							disabled={ruleCount === 1}
							aria-label={t("policy_group_remove_rule")}
							title={t("policy_group_remove_rule")}
						>
							<Icon name="X" className="size-3.5" />
						</Button>
					)}
				</div>

				<div className="rounded-lg bg-muted/50 px-3.5 py-3">
					<p className="mb-2.5 text-xs font-medium text-muted-foreground">
						{t("policy_group_rule_matcher_title")}
					</p>
					<div className="flex flex-wrap items-center gap-2 text-sm text-muted-foreground">
						<span>{t("policy_group_rule_matcher_prefix")}</span>
						<Input
							type="number"
							min="0"
							step="any"
							value={item.minFileSizeMb}
							onChange={(event) =>
								onRuleFieldChange(item.key, "minFileSizeMb", event.target.value)
							}
							placeholder={t("policy_group_size_unlimited")}
							className={`${ADMIN_CONTROL_HEIGHT_CLASS} h-8! w-24 px-2 text-center`}
							aria-label={t("policy_group_min_size_mb")}
						/>
						<span>{t("policy_group_rule_matcher_middle")}</span>
						<Input
							type="number"
							min="0"
							step="any"
							value={item.maxFileSizeMb}
							onChange={(event) =>
								onRuleFieldChange(item.key, "maxFileSizeMb", event.target.value)
							}
							placeholder={t("policy_group_size_unlimited")}
							className={`${ADMIN_CONTROL_HEIGHT_CLASS} h-8! w-24 px-2 text-center`}
							aria-label={t("policy_group_max_size_mb")}
						/>
						<span>{t("policy_group_rule_matcher_suffix")}</span>
					</div>
				</div>

				<div className="space-y-2.5">
					<p className="text-xs font-medium text-muted-foreground">
						{t("policy_group_rule_targets_title")}
					</p>

					{item.targets.map((target) => {
						const selectablePolicies = getSelectablePolicies(target.policyId);
						const selectablePolicyOptions = selectablePolicies.map(
							(policy) => ({
								label: formatPolicyTargetLabel(policy),
								value: String(policy.id),
							}),
						);
						const selectedPolicy = findPolicy(
							policies,
							target.policyId,
						);

						return (
							<div
								key={target.key}
								className="grid items-center gap-2.5 sm:grid-cols-[minmax(0,1fr)_104px_32px]"
							>
								<Select
									items={selectablePolicyOptions}
									value={target.policyId}
									onOpenChange={handlePolicySelectOpenChange}
									onValueChange={(value) =>
										setTarget(target.key, "policyId", value ?? "")
									}
								>
									<SelectTrigger
										className={`${ADMIN_CONTROL_HEIGHT_CLASS} w-full`}
									>
										<SelectValue placeholder={t("select_policy")}>
											{selectedPolicy
												? formatPolicyTarget(selectedPolicy)
												: target.policyId
													? `#${target.policyId}`
													: undefined}
										</SelectValue>
									</SelectTrigger>
									<SelectContent
										className="max-h-64"
										onScroll={handlePolicySelectScroll}
									>
										{selectablePolicies.map((policy) => (
											<SelectItem key={policy.id} value={String(policy.id)}>
												{formatPolicyTarget(policy)}
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
															? t("policy_group_loading_more_policies")
															: t("policy_group_scroll_to_load_more")}
													</SelectLabel>
												</SelectGroup>
											</>
										) : null}
									</SelectContent>
								</Select>
								<Input
									type="number"
									min="1"
									step="1"
									value={target.weight}
									onChange={(event) =>
										setTarget(target.key, "weight", event.target.value)
									}
									className={`${ADMIN_CONTROL_HEIGHT_CLASS} text-center`}
									aria-label={t("policy_group_target_weight")}
									title={t("policy_group_target_weight")}
								/>
								<Button
									type="button"
									variant="ghost"
									size="icon"
									className={`${ADMIN_ICON_BUTTON_CLASS} text-muted-foreground`}
									onClick={() => removeTarget(target.key)}
									disabled={item.targets.length === 1}
									aria-label={t("policy_group_target_remove")}
									title={t("policy_group_target_remove")}
								>
									<Icon name="X" className="size-3.5" />
								</Button>
								<div className="flex flex-wrap gap-x-4 gap-y-1 sm:col-span-3 sm:-mt-1">
									<span className="flex items-center gap-2 text-xs text-muted-foreground">
										<Switch
											size="sm"
											checked={target.isEnabled}
											onCheckedChange={(checked) =>
												setTarget(target.key, "isEnabled", checked)
											}
											aria-label={t("policy_group_target_enabled")}
										/>
										{t("policy_group_target_enabled")}
									</span>
									<span className="flex items-center gap-2 text-xs text-muted-foreground">
										<Switch
											size="sm"
											checked={target.acceptingNewWrites}
											onCheckedChange={(checked) =>
												setTarget(target.key, "acceptingNewWrites", checked)
											}
											aria-label={t("policy_group_target_accepting_new_writes")}
										/>
										{t("policy_group_target_accepting_new_writes")}
									</span>
								</div>
							</div>
						);
					})}

					<Button
						type="button"
						variant="ghost"
						size="sm"
						className="text-muted-foreground"
						onClick={addTarget}
					>
						<Icon name="Plus" className="mr-1 size-3.5" />
						{t("policy_group_add_target")}
					</Button>
					{item.selectionMode === "weighted_random" ? (
						<p className="text-xs text-muted-foreground">
							{t("policy_group_targets_weight_hint")}
						</p>
					) : null}
				</div>

				<div className="grid gap-4 border-t pt-3.5 sm:grid-cols-2">
					<div className="space-y-1.5">
						<Label htmlFor={`${item.key}-selection-mode`}>
							{t("policy_group_selection_mode")}
						</Label>
						<Select
							items={[
								{
									label: t("policy_group_selection_first_available"),
									value: "first_available",
								},
								{
									label: t("policy_group_selection_weighted_random"),
									value: "weighted_random",
								},
							]}
							value={item.selectionMode}
							onValueChange={(value) =>
								onRuleFieldChange(
									item.key,
									"selectionMode",
									(value ??
										"first_available") as PolicyGroupRuleForm["selectionMode"],
								)
							}
						>
							<SelectTrigger
								id={`${item.key}-selection-mode`}
								className={`${ADMIN_CONTROL_HEIGHT_CLASS} w-full`}
							>
								<SelectValue />
							</SelectTrigger>
							<SelectContent>
								<SelectItem value="first_available">
									{t("policy_group_selection_first_available")}
								</SelectItem>
								<SelectItem value="weighted_random">
									{t("policy_group_selection_weighted_random")}
								</SelectItem>
							</SelectContent>
						</Select>
					</div>
					<div className="space-y-1.5">
						<Label htmlFor={`${item.key}-unavailable-behavior`}>
							{t("policy_group_unavailable_behavior")}
						</Label>
						<Select
							items={[
								{
									label: t("policy_group_unavailable_next_rule"),
									value: "next_rule",
								},
								{
									label: t("policy_group_unavailable_reject"),
									value: "reject",
								},
							]}
							value={item.unavailableBehavior}
							onValueChange={(value) =>
								onRuleFieldChange(
									item.key,
									"unavailableBehavior",
									(value ??
										"next_rule") as PolicyGroupRuleForm["unavailableBehavior"],
								)
							}
						>
							<SelectTrigger
								id={`${item.key}-unavailable-behavior`}
								className={`${ADMIN_CONTROL_HEIGHT_CLASS} w-full`}
							>
								<SelectValue />
							</SelectTrigger>
							<SelectContent>
								<SelectItem value="next_rule">
									{t("policy_group_unavailable_next_rule")}
								</SelectItem>
								<SelectItem value="reject">
									{t("policy_group_unavailable_reject")}
								</SelectItem>
							</SelectContent>
						</Select>
					</div>
				</div>
			</div>
		</div>
	);
}

export function PolicyGroupEditorForm({
	mode,
	form,
	formError,
	hasMorePolicies,
	policies,
	policiesLoading,
	policiesLoadingMore,
	onAddRule,
	onFieldChange,
	onLoadMorePolicies,
	onMoveRule,
	onOpenSimulation,
	onRefreshPolicies,
	onRemoveRule,
	onReorderRule,
	onRuleFieldChange,
}: PolicyGroupEditorFormProps) {
	const { t } = useTranslation("admin");
	const [policySearch, setPolicySearch] = useState("");
	const [categoriesHelpOpen, setCategoriesHelpOpen] = useState(false);
	const [draggingRuleKey, setDraggingRuleKey] = useState<string | null>(null);
	const normalizedPolicySearch = policySearch.trim().toLowerCase();
	const filteredPolicies = policies.filter((policy) =>
		matchesPolicySearch(policy, normalizedPolicySearch),
	);

	// 规则重排 FLIP：动作驱动快照——只有重排动作（拖拽换位、箭头移动）
	// 在 setForm 前一刻当场记录各卡位置，渲染后把位置变化过的卡片从旧
	// 位置平滑滑到新位置。添加/删除/数据加载等其它 items 变化没有快照，
	// 一律不播（添加走展开动画，删除走收缩动画）。
	const ruleListRef = useRef<HTMLDivElement | null>(null);
	const flipSnapshotRef = useRef<Map<string, number> | null>(null);
	const flipTimersRef = useRef(new Map<string, ReturnType<typeof setTimeout>>());
	// 已渲染的规则 key：判断卡片是否为后续新增（首渲染不算新，不播展开）
	const renderedRuleKeysRef = useRef<Set<string> | null>(null);

	const captureRulePositions = () => {
		const snapshot = new Map<string, number>();
		const container = ruleListRef.current;
		if (!container) return snapshot;
		const scrollY = window.scrollY;
		for (const el of container.querySelectorAll<HTMLElement>(
			"[data-rule-key]",
		)) {
			const key = el.dataset.ruleKey;
			if (key) snapshot.set(key, el.getBoundingClientRect().top + scrollY);
		}
		return snapshot;
	};

	// biome-ignore lint/correctness/useExhaustiveDependencies: form.items 仅作重排后重新测量的触发信号，effect 内通过 ref 读取 DOM
	useLayoutEffect(() => {
		const snapshot = flipSnapshotRef.current;
		flipSnapshotRef.current = null;
		if (
			!snapshot ||
			window.matchMedia?.("(prefers-reduced-motion: reduce)").matches
		) {
			return;
		}
		const container = ruleListRef.current;
		if (!container) return;
		const scrollY = window.scrollY;
		for (const el of container.querySelectorAll<HTMLElement>(
			"[data-rule-key]",
		)) {
			const key = el.dataset.ruleKey;
			if (!key) continue;
			const oldTop = snapshot.get(key);
			if (oldTop == null) continue;
			const delta = oldTop - (el.getBoundingClientRect().top + scrollY);
			if (delta === 0) continue;
			// 连续重排时清掉上一段未播完的 timer，避免它提前还原 transition
			const pendingTimer = flipTimersRef.current.get(key);
			if (pendingTimer) clearTimeout(pendingTimer);
			el.style.transition = "none";
			el.style.transform = `translateY(${delta}px)`;
			void el.getBoundingClientRect(); // 强制 reflow，让 invert 先生效
			el.style.transition = "transform 180ms cubic-bezier(0.22, 1, 0.36, 1)";
			el.style.transform = "";
			flipTimersRef.current.set(
				key,
				setTimeout(() => {
					flipTimersRef.current.delete(key);
					// 播完清掉 inline transition，避免盖住卡片自身的 grid-rows 删除动画
					el.style.transition = "";
				}, 200),
			);
		}
	}, [form.items]);

	useEffect(() => {
		renderedRuleKeysRef.current = new Set(
			form.items.map((item) => item.key),
		);
	}, [form.items]);

	useEffect(() => {
		return () => {
			for (const timer of flipTimersRef.current.values()) {
				clearTimeout(timer);
			}
		};
	}, []);

	useEffect(() => {
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
		policiesLoading,
		policiesLoadingMore,
	]);

	return (
		<div className="grid gap-8 lg:grid-cols-[300px_minmax(0,1fr)]">
			{/* ── 左栏：基本信息 ── */}
			<aside className="animate-in fade-in slide-in-from-top-1 duration-200 fill-mode-backwards motion-reduce:animate-none space-y-4 lg:sticky lg:top-6 lg:self-start">
				<div className="space-y-4 rounded-xl bg-muted/30 p-4">
					<p className="text-[15px] font-semibold">
						{t("policy_group_basic_info")}
					</p>

					<div className="space-y-1.5">
						<Label htmlFor="policy-group-name">{t("core:name")}</Label>
						<Input
							id="policy-group-name"
							value={form.name}
							onChange={(event) => onFieldChange("name", event.target.value)}
							className={ADMIN_CONTROL_HEIGHT_CLASS}
							aria-invalid={!form.name.trim() ? true : undefined}
							required
						/>
					</div>

					<div className="space-y-1.5">
						<Label htmlFor="policy-group-description">
							{t("policy_group_description")}
							<span className="ml-1 font-normal text-muted-foreground/70">
								{t("policy_group_description_optional")}
							</span>
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

					<div className="space-y-4 border-t pt-4">
						<div className="flex items-start justify-between gap-3">
							<div className="space-y-0.5">
								<p className="text-sm font-medium">
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
						<div className="flex items-start justify-between gap-3">
							<div className="space-y-0.5">
								<p className="text-sm font-medium">
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
				</div>

				<div className="rounded-xl bg-muted/30 p-4">
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
							{form.isEnabled ? t("core:active") : t("core:disabled_status")}
						</Badge>
					</div>
				</div>
			</aside>

			{/* ── 右栏主区 ── */}
			<div className="min-w-0 space-y-8">
				{/* 准入 */}
				<section className="animate-in fade-in slide-in-from-top-1 duration-200 fill-mode-backwards motion-reduce:animate-none delay-75 space-y-5 rounded-xl bg-muted/30 p-5">
					<div>
						<h2 className="text-[15px] font-semibold">
							{t("policy_group_admission_title")}
						</h2>
						<p className="mt-1 text-xs text-muted-foreground">
							{t("policy_group_admission_desc")}
						</p>
					</div>

					<div className="flex items-start justify-between gap-3 rounded-lg border px-3.5 py-3">
						<div className="space-y-0.5">
							<p className="text-sm font-medium">
								{t("policy_group_extensionless")}
							</p>
							<p className="text-xs text-muted-foreground">
								{t("policy_group_extensionless_desc")}
							</p>
						</div>
						<Switch
							checked={form.admission?.accept_extensionless ?? true}
							onCheckedChange={(checked) =>
								onFieldChange("admission", {
									...(form.admission ?? {}),
									accept_extensionless: checked,
								})
							}
						/>
					</div>

					<div className="grid gap-4 sm:grid-cols-2">
						<div className="space-y-1.5">
							<Label htmlFor="policy-group-allowed-extensions">
								{t("policy_group_allowed_extensions")}
							</Label>
							<Input
								id="policy-group-allowed-extensions"
								value={form.admission?.allowed_extensions?.join(", ") ?? ""}
								placeholder="jpg, pdf, tar.gz"
								onChange={(event) =>
									onFieldChange("admission", {
										...(form.admission ?? {}),
										allowed_extensions: event.target.value
											.split(",")
											.map((value) => value.trim())
											.filter(Boolean),
									})
								}
								className={ADMIN_CONTROL_HEIGHT_CLASS}
							/>
							<p className="text-xs text-muted-foreground">
								{t("policy_group_allowed_extensions_desc")}
							</p>
						</div>
						<div className="space-y-1.5">
							<Label htmlFor="policy-group-denied-extensions">
								{t("policy_group_denied_extensions")}
							</Label>
							<Input
								id="policy-group-denied-extensions"
								value={form.admission?.denied_extensions?.join(", ") ?? ""}
								placeholder="exe, sh"
								onChange={(event) =>
									onFieldChange("admission", {
										...(form.admission ?? {}),
										denied_extensions: event.target.value
											.split(",")
											.map((value) => value.trim())
											.filter(Boolean),
									})
								}
								className={ADMIN_CONTROL_HEIGHT_CLASS}
							/>
							<p className="text-xs text-muted-foreground">
								{t("policy_group_denied_extensions_desc")}
							</p>
						</div>
						<div className="space-y-1.5">
							<Label htmlFor="policy-group-max-size">
								{t("policy_group_admission_max_size_mb")}
							</Label>
							<Input
								id="policy-group-max-size"
								type="number"
								min="0"
								step="any"
								value={bytesToMbInput(form.admission?.max_file_size ?? 0)}
								onChange={(event) =>
									onFieldChange("admission", {
										...(form.admission ?? {}),
										max_file_size: mbInputToBytes(event.target.value),
									})
								}
								className={ADMIN_CONTROL_HEIGHT_CLASS}
								placeholder={t("policy_group_size_unlimited")}
							/>
							<p className="text-xs text-muted-foreground">
								{t("policy_group_admission_max_size_desc")}
							</p>
						</div>
						<div className="space-y-1.5">
							<Label htmlFor="policy-group-execution-preference">
								{t("policy_group_execution_preference")}
							</Label>
							<Select
								items={[
									{
										label: t("policy_group_execution_automatic"),
										value: "automatic",
									},
									{
										label: t("policy_group_execution_server_stream"),
										value: "force_server_stream",
									},
								]}
								value={form.executionPreference ?? "automatic"}
								onValueChange={(value) =>
									onFieldChange(
										"executionPreference",
										(value ?? "automatic") as
											| "automatic"
											| "force_server_stream",
									)
								}
							>
								<SelectTrigger
									id="policy-group-execution-preference"
									className={`${ADMIN_CONTROL_HEIGHT_CLASS} w-full`}
								>
									<SelectValue />
								</SelectTrigger>
								<SelectContent>
									<SelectItem value="automatic">
										{t("policy_group_execution_automatic")}
									</SelectItem>
									<SelectItem value="force_server_stream">
										{t("policy_group_execution_server_stream")}
									</SelectItem>
								</SelectContent>
							</Select>
							<p
								className={
									form.executionPreference === "force_server_stream"
										? "text-xs text-amber-600 dark:text-amber-400"
										: "text-xs text-muted-foreground"
								}
							>
								{form.executionPreference === "force_server_stream"
									? t("policy_group_execution_server_stream_desc")
									: t("policy_group_execution_automatic_desc")}
							</p>
						</div>
					</div>

					<div className="border-t pt-4">
						<div className="mb-3 flex items-center">
							<p className="text-sm font-medium">
								{t("policy_group_category_section")}
							</p>
							<button
								type="button"
								className="ml-1.5 inline-flex size-4.5 items-center justify-center rounded-full border border-border text-[11px] text-muted-foreground hover:border-muted-foreground hover:text-foreground"
								onClick={() => setCategoriesHelpOpen(true)}
								aria-label={t("policy_group_categories_help_title")}
								title={t("policy_group_categories_help_title")}
							>
								?
							</button>
						</div>
						<div className="grid gap-5 lg:grid-cols-2">
							<CategoryCheckboxGroup
								legend={t("policy_group_allowed_categories")}
								hint={t("policy_group_allowed_categories_desc")}
								t={t}
								value={form.admission?.allowed_categories ?? []}
								onChange={(next) =>
									onFieldChange("admission", {
										...(form.admission ?? {}),
										allowed_categories: next,
									})
								}
							/>
							<CategoryCheckboxGroup
								legend={t("policy_group_denied_categories")}
								hint={t("policy_group_denied_categories_desc")}
								t={t}
								value={form.admission?.denied_categories ?? []}
								onChange={(next) =>
									onFieldChange("admission", {
										...(form.admission ?? {}),
										denied_categories: next,
									})
								}
							/>
						</div>
					</div>
				</section>

				{/* 规则 */}
				<section className="animate-in fade-in slide-in-from-top-1 duration-200 fill-mode-backwards motion-reduce:animate-none delay-150 space-y-4">
					<div className="flex items-start justify-between gap-4">
						<div>
							<h2 className="text-[15px] font-semibold">
								{t("policy_group_rules_title")}
							</h2>
							<p className="mt-1 max-w-2xl text-xs text-muted-foreground">
								{t("policy_group_rules_desc")}
							</p>
						</div>
						<div className="flex shrink-0 gap-2">
							{mode === "edit" && onOpenSimulation ? (
								<Button
									type="button"
									variant="outline"
									size="sm"
									className={ADMIN_CONTROL_HEIGHT_CLASS}
									onClick={onOpenSimulation}
								>
									<Icon name="Play" className="mr-1 size-3.5" />
									{t("policy_group_simulator_open")}
								</Button>
							) : null}
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
					</div>

					<div className="space-y-1.5">
						<Label htmlFor="policy-group-search">
							{t("policy_group_policy_search")}
						</Label>
						<Input
							id="policy-group-search"
							value={policySearch}
							onChange={(event) => setPolicySearch(event.target.value)}
							className={ADMIN_CONTROL_HEIGHT_CLASS}
							placeholder={t("policy_group_policy_search_placeholder")}
						/>
					</div>

					{policies.length === 0 ? (
						<div className="rounded-xl bg-muted/30 px-4 py-6 text-sm text-muted-foreground">
							{t("policy_group_no_policies_available")}
						</div>
					) : null}

					<div className="space-y-4" ref={ruleListRef}>
						{form.items.map((item, index) => (
							<RuleCard
								key={item.key}
								index={index}
								isDragging={draggingRuleKey === item.key}
							isNew={
								renderedRuleKeysRef.current != null &&
								!renderedRuleKeysRef.current.has(item.key)
							}
								item={item}
								policies={policies}
								filteredPolicies={filteredPolicies}
								hasMorePolicies={hasMorePolicies}
								policiesLoading={policiesLoading}
								policiesLoadingMore={policiesLoadingMore}
								ruleCount={form.items.length}
								t={t}
								onDragStart={setDraggingRuleKey}
								onDragEnd={() => setDraggingRuleKey(null)}
								onDragOverCard={(hoverIndex, after) => {
									if (!draggingRuleKey) return;
									const oldIndex = form.items.findIndex(
										(entry) => entry.key === draggingRuleKey,
									);
									if (oldIndex < 0) return;
									let target = after ? hoverIndex + 1 : hoverIndex;
									if (target > oldIndex) target -= 1;
									if (target === oldIndex) return;
									flipSnapshotRef.current = captureRulePositions();
									onReorderRule(draggingRuleKey, target);
								}}
								onLoadMorePolicies={onLoadMorePolicies}
								onMoveRule={(ruleKey, direction) => {
									flipSnapshotRef.current = captureRulePositions();
									onMoveRule(ruleKey, direction);
								}}
								onRefreshPolicies={onRefreshPolicies}
								onRemoveRule={onRemoveRule}
								onRuleFieldChange={onRuleFieldChange}
							/>
						))}
					</div>
				</section>

				{formError ? (
					<div className="rounded-xl bg-destructive/10 px-4 py-3 text-sm text-destructive">
						{formError}
					</div>
				) : null}
			</div>

			{/* 文件类别说明弹窗 */}
			<Dialog open={categoriesHelpOpen} onOpenChange={setCategoriesHelpOpen}>
				<DialogContent className="sm:max-w-lg">
					<DialogHeader>
						<DialogTitle>{t("policy_group_categories_help_title")}</DialogTitle>
						<DialogDescription>
							{t("policy_group_categories_help_intro")}
						</DialogDescription>
					</DialogHeader>
					<div className="divide-y text-sm">
						{FILE_CATEGORY_KEYS.map((category) => (
							<div
								key={category}
								className="grid grid-cols-[140px_minmax(0,1fr)] gap-3 py-2.5"
							>
								<div>
									<code className="text-xs text-blue-600 dark:text-blue-300">
										{category}
									</code>
									<span className="ml-1.5 text-muted-foreground">
										{t(`policy_group_category_${category}`)}
									</span>
								</div>
								<div className="text-muted-foreground">
									{t(`policy_group_category_examples_${category}`)}
								</div>
							</div>
						))}
					</div>
				</DialogContent>
			</Dialog>
		</div>
	);
}
