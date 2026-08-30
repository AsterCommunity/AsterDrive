import { useCallback, useEffect, useState } from "react";
import { useTranslation } from "react-i18next";
import { Navigate, useNavigate, useParams } from "react-router-dom";
import { toast } from "sonner";
import {
	PolicyGroupEditorForm,
	type PolicyLookup,
} from "@/components/admin/PolicyGroupEditorForm";
import { PolicyGroupSimulationDialog } from "@/components/admin/PolicyGroupSimulationDialog";
import {
	buildPolicyGroupPayload,
	buildPolicyGroupRuleForm,
	getDefaultPolicyGroupForm,
	getPolicyGroupForm,
	type PolicyGroupFormData,
	type PolicyGroupRuleForm,
	validatePolicyGroupForm,
} from "@/components/admin/policyGroupEditorShared";
import { AdminLayout } from "@/components/layout/AdminLayout";
import { AdminPageHeader } from "@/components/layout/AdminPageHeader";
import { AdminPageShell } from "@/components/layout/AdminPageShell";
import { Button } from "@/components/ui/button";
import { Icon } from "@/components/ui/icon";
import { getApiErrorMessage, handleApiError } from "@/hooks/useApiError";
import { usePageTitle } from "@/hooks/usePageTitle";
import { invalidateAdminPolicyGroupLookup } from "@/lib/adminPolicyGroupLookup";
import {
	loadAdminPolicyLookup,
	readAdminPolicyLookup,
} from "@/lib/adminPolicyLookup";
import { ADMIN_CONTROL_HEIGHT_CLASS } from "@/lib/constants";
import { adminPolicyGroupService } from "@/services/adminService";
import type {
	StoragePlacementSimulationResult,
	StoragePolicyGroup,
} from "@/types/api";

const POLICY_LOOKUP_PAGE_SIZE = 100;
const BYTES_PER_MB = 1024 * 1024;

function mergePolicies(
	current: PolicyLookup[],
	incoming: PolicyLookup[],
): PolicyLookup[] {
	if (incoming.length === 0) return current;
	const merged = [...current];
	const seen = new Set(current.map((policy) => policy.id));
	for (const policy of incoming) {
		if (seen.has(policy.id)) continue;
		seen.add(policy.id);
		merged.push(policy);
	}
	return merged;
}

export default function AdminPolicyGroupEditPage() {
	const { t } = useTranslation("admin");
	const navigate = useNavigate();
	const { groupId } = useParams<{ groupId?: string }>();
	const isCreate = groupId === "new";
	const parsedGroupId = Number(groupId);
	const isValidRoute =
		isCreate || (Number.isSafeInteger(parsedGroupId) && parsedGroupId > 0);

	usePageTitle(isCreate ? t("create_policy_group") : t("edit_policy_group"));

	const initialPolicies = readAdminPolicyLookup();
	const [policies, setPolicies] = useState<PolicyLookup[]>(
		initialPolicies ?? [],
	);
	const [loadedPoliciesCount, setLoadedPoliciesCount] = useState(
		initialPolicies?.length ?? 0,
	);
	const [policiesTotal, setPoliciesTotal] = useState(
		initialPolicies?.length ?? 0,
	);
	const [policiesLoading, setPoliciesLoading] = useState(
		initialPolicies == null,
	);
	const [policiesLoadingMore, setPoliciesLoadingMore] = useState(false);

	const [group, setGroup] = useState<StoragePolicyGroup | null>(null);
	const [groupLoading, setGroupLoading] = useState(!isCreate);
	const [groupNotFound, setGroupNotFound] = useState(false);

	const [form, setForm] = useState<PolicyGroupFormData>(() =>
		getDefaultPolicyGroupForm(initialPolicies ?? []),
	);
	const [formError, setFormError] = useState<string | null>(null);
	const [submitting, setSubmitting] = useState(false);

	const [simulationOpen, setSimulationOpen] = useState(false);
	const [simulationFilename, setSimulationFilename] = useState("example.pdf");
	const [simulationFileSizeMb, setSimulationFileSizeMb] = useState("1");
	const [simulationMimeType, setSimulationMimeType] = useState(
		"application/octet-stream",
	);
	const [simulationFolderPolicyId, setSimulationFolderPolicyId] = useState("");
	const [simulationResult, setSimulationResult] =
		useState<StoragePlacementSimulationResult | null>(null);
	const [simulationError, setSimulationError] = useState<string | null>(null);
	const [simulationSubmitting, setSimulationSubmitting] = useState(false);

	const hasMorePolicies = loadedPoliciesCount < policiesTotal;

	const loadPolicies = useCallback(
		async ({ force = false }: { force?: boolean } = {}) => {
			try {
				setPoliciesLoading(true);
				setPoliciesLoadingMore(false);
				const policyLookup = await loadAdminPolicyLookup({
					force,
					limit: POLICY_LOOKUP_PAGE_SIZE,
				});
				setPoliciesTotal(policyLookup.length);
				setLoadedPoliciesCount(policyLookup.length);
				setPolicies(policyLookup);
			} catch (e) {
				handleApiError(e);
			} finally {
				setPoliciesLoading(false);
				setPoliciesLoadingMore(false);
			}
		},
		[],
	);

	useEffect(() => {
		void loadPolicies();
	}, [loadPolicies]);

	// 创建模式下表单先于策略列表初始化；策略就绪后给未选择策略的目标行
	// 补上默认策略（仅当仍是初始的单一空目标，避免覆盖用户手动操作）。
	useEffect(() => {
		if (!isCreate || policies.length === 0) return;
		setForm((prev) => {
			if (prev.items.length !== 1) return prev;
			const [onlyItem] = prev.items;
			if (
				onlyItem.targets.length !== 1 ||
				onlyItem.targets[0].policyId !== ""
			) {
				return prev;
			}
			return {
				...prev,
				items: [
					{
						...onlyItem,
						targets: [
							{
								...onlyItem.targets[0],
								policyId: String(policies[0].id),
							},
						],
					},
				],
			};
		});
	}, [isCreate, policies]);

	useEffect(() => {
		if (isCreate || !isValidRoute) return;

		let cancelled = false;
		setGroupLoading(true);
		adminPolicyGroupService
			.get(parsedGroupId)
			.then((loadedGroup) => {
				if (cancelled) return;
				setGroup(loadedGroup);
				setPolicies((prev) =>
					mergePolicies(
						prev,
						loadedGroup.rules.flatMap((rule) =>
							rule.targets.map((target) => target.policy),
						),
					),
				);
				setForm(getPolicyGroupForm(loadedGroup));
			})
			.catch(() => {
				if (!cancelled) {
					setGroupNotFound(true);
				}
			})
			.finally(() => {
				if (!cancelled) {
					setGroupLoading(false);
				}
			});

		return () => {
			cancelled = true;
		};
	}, [isCreate, isValidRoute, parsedGroupId]);

	const loadMorePolicies = useCallback(async () => {
		if (policiesLoading || policiesLoadingMore || !hasMorePolicies) {
			return;
		}
		await loadPolicies();
	}, [hasMorePolicies, loadPolicies, policiesLoading, policiesLoadingMore]);

	const setField = <K extends keyof PolicyGroupFormData>(
		key: K,
		value: PolicyGroupFormData[K],
	) => {
		setForm((prev) => ({ ...prev, [key]: value }));
		setFormError(null);
	};

	const setRuleField = <K extends Exclude<keyof PolicyGroupRuleForm, "key">>(
		ruleKey: string,
		key: K,
		value: PolicyGroupRuleForm[K],
	) => {
		setForm((prev) => ({
			...prev,
			items: prev.items.map((item) =>
				item.key === ruleKey ? { ...item, [key]: value } : item,
			),
		}));
		setFormError(null);
	};

	const getNextPolicyId = () => {
		const selected = new Set(
			form.items.flatMap((item) =>
				item.targets.map((target) => target.policyId),
			),
		);
		return (
			policies.find((policy) => !selected.has(String(policy.id)))?.id ??
			policies[0]?.id ??
			null
		);
	};

	const addRule = () => {
		setForm((prev) => ({
			...prev,
			items: [...prev.items, buildPolicyGroupRuleForm(getNextPolicyId())],
		}));
		setFormError(null);
	};

	const removeRule = (ruleKey: string) => {
		setForm((prev) => ({
			...prev,
			items: prev.items.filter((item) => item.key !== ruleKey),
		}));
		setFormError(null);
	};

	const moveRule = (ruleKey: string, direction: -1 | 1) => {
		setForm((prev) => {
			const index = prev.items.findIndex((item) => item.key === ruleKey);
			const targetIndex = index + direction;
			if (index < 0 || targetIndex < 0 || targetIndex >= prev.items.length) {
				return prev;
			}
			const items = [...prev.items];
			const [moved] = items.splice(index, 1);
			items.splice(targetIndex, 0, moved);
			return { ...prev, items };
		});
		setFormError(null);
	};

	const reorderRule = (ruleKey: string, targetIndex: number) => {
		setForm((prev) => {
			const index = prev.items.findIndex((item) => item.key === ruleKey);
			if (index < 0 || index === targetIndex) {
				return prev;
			}
			const items = [...prev.items];
			const [moved] = items.splice(index, 1);
			items.splice(targetIndex, 0, moved);
			return { ...prev, items };
		});
		setFormError(null);
	};

	const resetSimulationState = () => {
		setSimulationFilename("example.pdf");
		setSimulationFileSizeMb("1");
		setSimulationMimeType("application/octet-stream");
		setSimulationFolderPolicyId("");
		setSimulationResult(null);
		setSimulationError(null);
		setSimulationSubmitting(false);
	};

	const handleSimulationInputChange = (
		setter: (value: string) => void,
		value: string,
	) => {
		setter(value);
		setSimulationResult(null);
		setSimulationError(null);
	};

	const runSimulation = async () => {
		if (!group) return;
		const filename = simulationFilename.trim();
		if (!filename) {
			setSimulationError(t("policy_group_simulator_filename_required"));
			return;
		}
		const fileSizeMb = Number(simulationFileSizeMb);
		const fileSize = Math.round(fileSizeMb * BYTES_PER_MB);
		if (
			!Number.isFinite(fileSizeMb) ||
			fileSizeMb < 0 ||
			!Number.isSafeInteger(fileSize)
		) {
			setSimulationError(t("policy_group_simulator_size_invalid"));
			return;
		}
		const folderPolicyId = simulationFolderPolicyId
			? Number(simulationFolderPolicyId)
			: null;
		if (folderPolicyId !== null && !Number.isSafeInteger(folderPolicyId)) {
			setSimulationError(t("policy_group_simulator_folder_policy_invalid"));
			return;
		}

		try {
			setSimulationSubmitting(true);
			setSimulationError(null);
			setSimulationResult(
				await adminPolicyGroupService.simulate(group.id, {
					filename,
					file_size: fileSize,
					mime_type: simulationMimeType.trim(),
					folder_policy_id: folderPolicyId,
				}),
			);
		} catch (error) {
			setSimulationResult(null);
			setSimulationError(getApiErrorMessage(error));
		} finally {
			setSimulationSubmitting(false);
		}
	};

	const backToList = () => {
		navigate("/admin/policy-groups", { viewTransition: false });
	};

	const submitForm = async () => {
		const validationError = validatePolicyGroupForm(form, policies.length, t);
		if (validationError) {
			setFormError(validationError);
			return;
		}

		const payload = buildPolicyGroupPayload(form);

		try {
			setSubmitting(true);
			if (isCreate) {
				await adminPolicyGroupService.create(payload);
				toast.success(t("policy_group_created"));
			} else {
				await adminPolicyGroupService.update(parsedGroupId, payload);
				toast.success(t("policy_group_updated"));
			}
			invalidateAdminPolicyGroupLookup();
			backToList();
		} catch (e) {
			handleApiError(e);
		} finally {
			setSubmitting(false);
		}
	};

	if (!isValidRoute) {
		return <Navigate to="/admin/policy-groups" replace />;
	}

	if (groupNotFound) {
		return (
			<AdminLayout>
				<AdminPageShell>
					<div className="flex flex-col items-center gap-4 py-16 text-center">
						<p className="text-sm text-muted-foreground">
							{t("policy_group_not_found")}
						</p>
						<Button
							variant="outline"
							size="sm"
							className={ADMIN_CONTROL_HEIGHT_CLASS}
							onClick={backToList}
						>
							<Icon name="ArrowLeft" className="mr-1 size-4" />
							{t("policy_group_back_to_list")}
						</Button>
					</div>
				</AdminPageShell>
			</AdminLayout>
		);
	}

	const pageTitle = isCreate
		? t("create_policy_group")
		: (group?.name ?? t("edit_policy_group"));

	return (
		<AdminLayout>
			<AdminPageShell>
				<form
					autoComplete="off"
					onSubmit={(event) => {
						event.preventDefault();
						void submitForm();
					}}
				>
					<div className="animate-in fade-in slide-in-from-top-1 duration-200 fill-mode-backwards motion-reduce:animate-none mb-2">
						<Button
							type="button"
							variant="ghost"
							size="sm"
							className="-ml-2 text-muted-foreground"
							onClick={backToList}
						>
							<Icon name="ArrowLeft" className="mr-1 size-4" />
							{t("policy_group_back_to_list")}
						</Button>
					</div>
					<AdminPageHeader
						className="animate-in fade-in slide-in-from-top-1 duration-200 fill-mode-backwards motion-reduce:animate-none px-0 md:px-0"
						title={pageTitle}
						description={t("policy_group_page_desc")}
						actions={
							<>
								<Button
									type="button"
									variant="outline"
									size="sm"
									className={ADMIN_CONTROL_HEIGHT_CLASS}
									onClick={backToList}
								>
									{t("core:cancel")}
								</Button>
								<Button
									type="submit"
									size="sm"
									className={ADMIN_CONTROL_HEIGHT_CLASS}
									disabled={
										submitting ||
										groupLoading ||
										policiesLoading ||
										policies.length === 0
									}
								>
									{submitting ? (
										<Icon name="Spinner" className="mr-1 size-4 animate-spin" />
									) : (
										<Icon name="FloppyDisk" className="mr-1 size-4" />
									)}
									{isCreate ? t("core:create") : t("save_changes")}
								</Button>
							</>
						}
					/>

					{groupLoading ? (
						<div className="flex items-center justify-center gap-2 py-16 text-sm text-muted-foreground">
							<Icon name="Spinner" className="size-4 animate-spin" />
							{t("core:loading")}
						</div>
					) : (
						<PolicyGroupEditorForm
							mode={isCreate ? "create" : "edit"}
							form={form}
							formError={formError}
							hasMorePolicies={hasMorePolicies}
							policies={policies}
							policiesLoading={policiesLoading}
							policiesLoadingMore={policiesLoadingMore}
							onAddRule={addRule}
							onFieldChange={setField}
							onLoadMorePolicies={loadMorePolicies}
							onMoveRule={moveRule}
							onOpenSimulation={
								isCreate
									? undefined
									: () => {
											resetSimulationState();
											setSimulationOpen(true);
										}
							}
							onRefreshPolicies={loadPolicies}
							onRemoveRule={removeRule}
							onReorderRule={reorderRule}
							onRuleFieldChange={setRuleField}
						/>
					)}

					{!groupLoading && (
						<div className="mt-8 flex justify-end gap-2 border-t pt-5">
							<Button
								type="button"
								variant="outline"
								className={ADMIN_CONTROL_HEIGHT_CLASS}
								onClick={backToList}
							>
								{t("core:cancel")}
							</Button>
							<Button
								type="submit"
								className={ADMIN_CONTROL_HEIGHT_CLASS}
								disabled={
									submitting || policiesLoading || policies.length === 0
								}
							>
								{submitting ? (
									<Icon name="Spinner" className="mr-1 size-4 animate-spin" />
								) : (
									<Icon name="FloppyDisk" className="mr-1 size-4" />
								)}
								{isCreate ? t("core:create") : t("save_changes")}
							</Button>
						</div>
					)}
				</form>

				<PolicyGroupSimulationDialog
					open={simulationOpen}
					group={group}
					policies={policies}
					filename={simulationFilename}
					fileSizeMb={simulationFileSizeMb}
					mimeType={simulationMimeType}
					folderPolicyId={simulationFolderPolicyId}
					result={simulationResult}
					error={simulationError}
					submitting={simulationSubmitting}
					onOpenChange={(open) => {
						setSimulationOpen(open);
						if (!open) {
							resetSimulationState();
						}
					}}
					onFilenameChange={(value) =>
						handleSimulationInputChange(setSimulationFilename, value)
					}
					onFileSizeMbChange={(value) =>
						handleSimulationInputChange(setSimulationFileSizeMb, value)
					}
					onMimeTypeChange={(value) =>
						handleSimulationInputChange(setSimulationMimeType, value)
					}
					onFolderPolicyIdChange={(value) =>
						handleSimulationInputChange(setSimulationFolderPolicyId, value)
					}
					onSimulate={() => void runSimulation()}
				/>
			</AdminPageShell>
		</AdminLayout>
	);
}
