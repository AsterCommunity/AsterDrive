import type { TFunction } from "i18next";
import { InlineConfirm } from "@/components/common/ManagerDialogShell";
import { Button } from "@/components/ui/button";
import { Icon } from "@/components/ui/icon";
import { translateStorageConnectorMessage } from "@/lib/adminStorageConnectorLocalizations";
import type { StorageConnectorTransitionPreview } from "@/types/api";

export function storageConnectorTransitionKey(
	transition: StorageConnectorTransitionPreview,
) {
	return `${transition.target_connector_id}:${transition.transition_id}`;
}

export function StorageConnectorTransitionPanel({
	confirmKey,
	loading,
	mode,
	submittingKey,
	t,
	transitions,
	unsavedChanges,
	onCancel,
	onConfirm,
	onRequest,
}: {
	confirmKey: string | null;
	loading: boolean;
	mode: "create" | "edit";
	submittingKey: string | null;
	t: TFunction;
	transitions: StorageConnectorTransitionPreview[];
	unsavedChanges: boolean;
	onCancel: () => void;
	onConfirm: (transition: StorageConnectorTransitionPreview) => void;
	onRequest: (transition: StorageConnectorTransitionPreview) => void;
}) {
	if (!loading && transitions.length === 0) {
		return null;
	}

	return (
		<section
			data-testid="storage-connector-transition-panel"
			className="rounded-xl border border-primary/25 bg-primary/[0.04] p-4"
		>
			<div className="flex items-start gap-3">
				<div className="mt-0.5 flex size-8 shrink-0 items-center justify-center rounded-lg bg-primary/10 text-primary">
					<Icon
						name={loading ? "Spinner" : "ArrowsClockwise"}
						className={`size-4 ${loading ? "animate-spin" : ""}`}
					/>
				</div>
				<div className="min-w-0 flex-1 space-y-3">
					<div>
						<h4 className="text-sm font-semibold">
							{t("policy_connector_transition_title")}
						</h4>
						<p className="mt-1 text-xs leading-5 text-muted-foreground">
							{mode === "create"
								? t("policy_connector_transition_draft_desc")
								: t("policy_connector_transition_saved_desc")}
						</p>
					</div>
					{transitions.map((transition) => {
						const key = storageConnectorTransitionKey(transition);
						const connectorT = (
							messageKey: string,
							values?: Record<string, number | string>,
						) =>
							translateStorageConnectorMessage(
								t,
								transition.target_connector_id,
								messageKey,
								values,
							);
						const submitting = submittingKey === key;
						const confirming = confirmKey === key;
						return (
							<div key={key} className="rounded-lg border bg-background/80 p-3">
								<div className="flex flex-col gap-3 sm:flex-row sm:items-start sm:justify-between">
									<div className="min-w-0">
										<p className="text-sm font-medium">
											{connectorT(transition.label_key)}
										</p>
										<p className="mt-1 text-xs leading-5 text-muted-foreground">
											{connectorT(transition.description_key)}
										</p>
										{mode === "edit" && unsavedChanges ? (
											<p className="mt-1 text-xs font-medium text-amber-700 dark:text-amber-300">
												{t("policy_connector_transition_save_first")}
											</p>
										) : null}
									</div>
									<Button
										type="button"
										size="sm"
										variant="outline"
										disabled={submitting || (mode === "edit" && unsavedChanges)}
										onClick={() => onRequest(transition)}
									>
										{submitting ? (
											<Icon
												name="Spinner"
												className="mr-1 size-3.5 animate-spin"
											/>
										) : null}
										{mode === "create"
											? t("policy_connector_transition_apply")
											: t("policy_connector_transition_execute")}
									</Button>
								</div>
								{confirming ? (
									<InlineConfirm className="mt-3">
										<div className="flex flex-col gap-3 sm:flex-row sm:items-center sm:justify-between">
											<p className="text-xs text-muted-foreground">
												{mode === "create"
													? t("policy_connector_transition_draft_confirm")
													: t("policy_connector_transition_saved_confirm")}
											</p>
											<div className="flex shrink-0 gap-2">
												<Button
													type="button"
													size="sm"
													variant="outline"
													onClick={onCancel}
												>
													{t("core:cancel")}
												</Button>
												<Button
													type="button"
													size="sm"
													onClick={() => onConfirm(transition)}
												>
													{t("core:confirm")}
												</Button>
											</div>
										</div>
									</InlineConfirm>
								) : null}
							</div>
						);
					})}
				</div>
			</div>
		</section>
	);
}
