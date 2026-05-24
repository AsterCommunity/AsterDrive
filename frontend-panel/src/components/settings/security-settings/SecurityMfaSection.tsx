import { useCallback, useEffect, useReducer, useState } from "react";
import { useTranslation } from "react-i18next";
import { toast } from "sonner";
import { ConfirmDialog } from "@/components/common/ConfirmDialog";
import { handleApiError } from "@/hooks/useApiError";
import { useConfirmDialog } from "@/hooks/useConfirmDialog";
import { writeTextToClipboard } from "@/lib/clipboard";
import { authService, type MfaStatus } from "@/services/authService";
import {
	actionReducer,
	createSetupState,
	downloadRecoveryCodes,
	EMPTY_ACTION_STATE,
	formatRecoveryCodesFile,
	type PendingAction,
	setupReducer,
	stepIndex,
} from "./mfaTypes";
import { SecurityMfaHeader } from "./SecurityMfaHeader";
import { SecurityMfaSensitiveActionForm } from "./SecurityMfaSensitiveActionForm";
import { SecurityMfaSetupPanel } from "./SecurityMfaSetupPanel";
import {
	SecurityMfaEmptyState,
	SecurityMfaStatusCard,
} from "./SecurityMfaStatusCard";

export function SecurityMfaSection() {
	const { t } = useTranslation(["auth", "core", "settings"]);
	const [status, setStatus] = useState<MfaStatus | null>(null);
	const [loading, setLoading] = useState(false);
	const [setupState, dispatchSetup] = useReducer(setupReducer, undefined, () =>
		createSetupState(),
	);
	const [actionState, dispatchAction] = useReducer(
		actionReducer,
		EMPTY_ACTION_STATE,
	);

	const load = useCallback(async () => {
		try {
			setLoading(true);
			setStatus(await authService.getMfaStatus());
		} catch (error) {
			handleApiError(error);
		} finally {
			setLoading(false);
		}
	}, []);

	useEffect(() => {
		void load();
	}, [load]);

	const copy = async (value: string, message?: string) => {
		try {
			await writeTextToClipboard(value);
			toast.success(message ?? t("core:copied_to_clipboard"));
		} catch (error) {
			handleApiError(error);
		}
	};

	const cancelSetup = () => {
		dispatchSetup({ type: "reset" });
	};

	const startSetup = async () => {
		try {
			dispatchSetup({ type: "start_busy" });
			const nextSetup = await authService.startTotpSetup();
			dispatchSetup({ type: "start_success", setup: nextSetup });
		} catch (error) {
			handleApiError(error);
			dispatchSetup({ type: "start_error" });
		}
	};

	const finishSetup = async () => {
		if (!setupState.setup) return;
		try {
			dispatchSetup({ type: "finish_busy" });
			const result = await authService.finishTotpSetup({
				flow_token: setupState.setup.flow_token,
				code: setupState.code,
				name: setupState.name.trim() || undefined,
			});
			dispatchSetup({
				type: "finish_success",
				recoveryCodes: result.recovery_codes,
			});
			await load();
			toast.success(t("settings:settings_mfa_enabled"));
		} catch (error) {
			handleApiError(error);
			dispatchSetup({ type: "finish_error" });
		}
	};

	const completeSetup = () => {
		if (!setupState.recoveryConfirmed) return;
		cancelSetup();
	};

	const submitSensitiveAction = async () => {
		if (!actionState.kind) return;
		try {
			dispatchAction({ type: "busy", busy: true });
			if (actionState.kind === "disable") {
				const factor = status?.factors[0];
				if (factor) {
					await authService.deleteMfaFactor(factor.id, {
						code: actionState.code,
					});
					cancelSetup();
					toast.success(t("settings:settings_mfa_disabled"));
				}
			} else {
				const codes = await authService.regenerateMfaRecoveryCodes({
					code: actionState.code,
				});
				dispatchSetup({
					type: "finish_success",
					recoveryCodes: codes,
				});
				toast.success(t("settings:settings_mfa_recovery_regenerated"));
			}
			dispatchAction({ type: "reset" });
			await load();
		} catch (error) {
			handleApiError(error);
		} finally {
			dispatchAction({ type: "busy", busy: false });
		}
	};

	const { requestConfirm, dialogProps } = useConfirmDialog<PendingAction>(
		(nextAction) => {
			if (!nextAction) return;
			dispatchAction({ type: "open", kind: nextAction });
		},
	);

	const enabled = status?.enabled ?? false;
	const factor = status?.factors[0] ?? null;
	const confirmAction = dialogProps.confirmId;
	const activeStep = setupState.step ?? "intro";
	const activeStepIndex = stepIndex(activeStep);
	const canFinishSetup =
		!!setupState.setup &&
		!setupState.finishBusy &&
		setupState.code.trim().length === 6;
	const confirmRecoverySaved = () => {
		if (!setupState.recoveryConfirmed) {
			dispatchSetup({ type: "toggle_recovery_confirmed" });
		}
	};
	const recoveryCodesFile = () =>
		formatRecoveryCodesFile(setupState.recoveryCodes);

	return (
		<div className="space-y-5">
			<SecurityMfaHeader
				enabled={enabled}
				loading={loading}
				onRefresh={() => void load()}
			/>

			{factor ? (
				<SecurityMfaStatusCard
					factor={factor}
					recoveryCodesRemaining={status?.recovery_codes_remaining ?? 0}
					onRequestConfirm={requestConfirm}
				/>
			) : setupState.step ? null : (
				<SecurityMfaEmptyState
					onStartSetup={() => dispatchSetup({ type: "intro" })}
				/>
			)}

			{setupState.step ? (
				<SecurityMfaSetupPanel
					activeStep={activeStep}
					activeStepIndex={activeStepIndex}
					canFinishSetup={canFinishSetup}
					setupState={setupState}
					onBackToIntro={() =>
						dispatchSetup({ type: "set_step", step: "intro" })
					}
					onBackToScan={() => dispatchSetup({ type: "set_step", step: "scan" })}
					onCancel={cancelSetup}
					onCodeChange={(code) => dispatchSetup({ type: "set_code", code })}
					onContinueToVerify={() =>
						dispatchSetup({ type: "set_step", step: "verify" })
					}
					onCopy={copy}
					onCopyRecoveryCodes={() =>
						void (async () => {
							await copy(
								recoveryCodesFile(),
								t("settings:settings_mfa_recovery_copied"),
							);
							confirmRecoverySaved();
						})()
					}
					onDownloadRecoveryCodes={() => {
						downloadRecoveryCodes(recoveryCodesFile());
						confirmRecoverySaved();
					}}
					onFinish={() => void finishSetup()}
					onIntroContinue={() => void startSetup()}
					onNameChange={(name) => dispatchSetup({ type: "set_name", name })}
					onRecoveryConfirmChange={() =>
						dispatchSetup({ type: "toggle_recovery_confirmed" })
					}
					onRecoveryDone={completeSetup}
					onToggleSecret={() => dispatchSetup({ type: "toggle_secret" })}
				/>
			) : null}

			<SecurityMfaSensitiveActionForm
				actionState={actionState}
				onCancel={() => dispatchAction({ type: "reset" })}
				onCodeChange={(code) => dispatchAction({ type: "code", code })}
				onSubmit={() => void submitSensitiveAction()}
			/>

			<ConfirmDialog
				{...dialogProps}
				title={
					confirmAction === "disable"
						? t("settings:settings_mfa_disable_title")
						: t("settings:settings_mfa_regenerate_title")
				}
				description={
					confirmAction === "disable"
						? t("settings:settings_mfa_disable_desc")
						: t("settings:settings_mfa_regenerate_desc")
				}
				confirmLabel={t("core:continue")}
				variant={confirmAction === "disable" ? "destructive" : "default"}
			/>
		</div>
	);
}
