import { useRef } from "react";
import { AnimatedCollapsible } from "@/components/common/AnimatedCollapsible";
import { InlineConfirm } from "@/components/common/ManagerDialogShell";
import { Button } from "@/components/ui/button";
import { Icon } from "@/components/ui/icon";
import { translateStorageConnectorMessage } from "@/lib/adminStorageConnectorLocalizations";
import { ADMIN_CONTROL_HEIGHT_CLASS } from "@/lib/constants";
import {
	type StorageConnectorPromotionCandidate,
	storageConnectorPromotionKey,
} from "./policyPromotion";
import type { Translate } from "./StoragePolicyFieldTypes";

interface StorageConnectorPromotionPanelProps {
	blocked: boolean;
	candidates: StorageConnectorPromotionCandidate[];
	confirmKey: string | null;
	mode: "create" | "edit";
	submittingKey: string | null;
	t: Translate;
	onApplyDraft: (candidate: StorageConnectorPromotionCandidate) => void;
	onCancel: () => void;
	onConfirm: (candidate: StorageConnectorPromotionCandidate) => void;
	onRequest: (candidate: StorageConnectorPromotionCandidate) => void;
}

export function StorageConnectorPromotionPanel({
	blocked,
	candidates,
	confirmKey,
	mode,
	submittingKey,
	t,
	onApplyDraft,
	onCancel,
	onConfirm,
	onRequest,
}: StorageConnectorPromotionPanelProps) {
	const renderedCandidatesRef = useRef(candidates);
	if (candidates.length > 0) {
		renderedCandidatesRef.current = candidates;
	}
	const renderedCandidates =
		candidates.length > 0 ? candidates : renderedCandidatesRef.current;

	return (
		<AnimatedCollapsible open={candidates.length > 0}>
			<section className="space-y-3 rounded-lg border border-primary/25 bg-primary/5 p-4">
				<div className="flex items-center gap-2 text-sm font-medium">
					<Icon name="ArrowUp" className="size-4 text-primary" />
					<span>{t("policy_connector_promotion_title")}</span>
				</div>
				{renderedCandidates.map((candidate) => {
					const key = storageConnectorPromotionKey(candidate);
					const connectorT: Translate = (messageKey, values) =>
						translateStorageConnectorMessage(
							t,
							candidate.targetDescriptor.connector_id,
							messageKey,
							values,
						);
					const targetLabel = connectorT(
						candidate.targetDescriptor.ui.label_key,
					);
					const submitting = submittingKey === key;
					const confirmOpen = confirmKey === key;
					return (
						<div
							key={key}
							className="space-y-3 border-t border-primary/15 pt-3 first:border-t-0 first:pt-0"
						>
							<div className="flex flex-col gap-3 sm:flex-row sm:items-start sm:justify-between">
								<div className="min-w-0 space-y-1">
									<p className="text-sm font-medium">{targetLabel}</p>
									<p className="text-xs leading-5 text-muted-foreground">
										{connectorT(candidate.promotion.description_key)}
									</p>
									{mode === "edit" && blocked ? (
										<p className="text-xs leading-5 text-amber-700 dark:text-amber-300">
											{t("policy_connector_promotion_unsaved_blocked")}
										</p>
									) : null}
								</div>
								<Button
									type="button"
									variant="outline"
									className={ADMIN_CONTROL_HEIGHT_CLASS}
									disabled={
										submitting || confirmOpen || (mode === "edit" && blocked)
									}
									onClick={() =>
										mode === "create"
											? onApplyDraft(candidate)
											: onRequest(candidate)
									}
								>
									{submitting ? (
										<Icon
											name="Spinner"
											className="mr-1 size-3.5 animate-spin"
										/>
									) : null}
									{t(
										mode === "create"
											? "policy_connector_promotion_use_draft"
											: "policy_connector_promotion_action",
										{ connector: targetLabel },
									)}
								</Button>
							</div>
							<AnimatedCollapsible open={confirmOpen} contentClassName="pt-3">
								<InlineConfirm>
									<div className="flex flex-col gap-3 sm:flex-row sm:items-center sm:justify-between">
										<div>
											<p className="text-sm font-medium">
												{t("policy_connector_promotion_confirm_title", {
													connector: targetLabel,
												})}
											</p>
											<p className="mt-1 text-xs leading-5 text-muted-foreground">
												{connectorT(candidate.promotion.confirmation_key)}
											</p>
										</div>
										<div className="flex shrink-0 items-center gap-2">
											<Button
												type="button"
												variant="outline"
												className={ADMIN_CONTROL_HEIGHT_CLASS}
												disabled={submitting}
												onClick={onCancel}
											>
												{t("core:cancel")}
											</Button>
											<Button
												type="button"
												className={ADMIN_CONTROL_HEIGHT_CLASS}
												disabled={submitting}
												onClick={() => onConfirm(candidate)}
											>
												{submitting ? (
													<Icon
														name="Spinner"
														className="mr-1 size-3.5 animate-spin"
													/>
												) : null}
												{t("policy_connector_promotion_confirm")}
											</Button>
										</div>
									</div>
								</InlineConfirm>
							</AnimatedCollapsible>
						</div>
					);
				})}
			</section>
		</AnimatedCollapsible>
	);
}
