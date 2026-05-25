import QRCode from "qrcode";
import { useEffect, useMemo, useRef } from "react";
import { useTranslation } from "react-i18next";
import { Button } from "@/components/ui/button";
import { Icon } from "@/components/ui/icon";
import { Input } from "@/components/ui/input";
import { Label } from "@/components/ui/label";
import { cn } from "@/lib/utils";
import type { TotpSetupStartResponse } from "@/services/authService";
import {
	SETUP_STEPS,
	type SetupStep,
	type SetupUiState,
	stepIndex,
} from "./mfaTypes";
import { SecurityMfaMeasuredMotion } from "./SecurityMfaMotion";
import { SecurityMfaStepMotion } from "./SecurityMfaStepMotion";

type QrModules = ReturnType<typeof QRCode.create>["modules"];

interface SecurityMfaSetupPanelProps {
	activeStep: SetupStep;
	activeStepIndex: number;
	canFinishSetup: boolean;
	setupState: SetupUiState;
	onBackToIntro: () => void;
	onBackToScan: () => void;
	onCancel: () => void;
	onCodeChange: (value: string) => void;
	onContinueToVerify: () => void;
	onCopy: (value: string, message?: string) => void;
	onCopyRecoveryCodes: () => void;
	onDownloadRecoveryCodes: () => void;
	onFinish: () => void;
	onIntroContinue: () => void;
	onNameChange: (value: string) => void;
	onRecoveryConfirmChange: () => void;
	onRecoveryDone: () => void;
	onToggleSecret: () => void;
}

export function SecurityMfaSetupPanel({
	activeStep,
	activeStepIndex,
	canFinishSetup,
	onBackToIntro,
	onBackToScan,
	onCancel,
	onCodeChange,
	onContinueToVerify,
	onCopy,
	onCopyRecoveryCodes,
	onDownloadRecoveryCodes,
	onFinish,
	onIntroContinue,
	onNameChange,
	onRecoveryConfirmChange,
	onRecoveryDone,
	onToggleSecret,
	setupState,
}: SecurityMfaSetupPanelProps) {
	const { t } = useTranslation(["core", "settings"]);
	const previousStepIndexRef = useRef(activeStepIndex);
	const stepDirection =
		activeStepIndex >= previousStepIndexRef.current ? "forward" : "backward";

	useEffect(() => {
		previousStepIndexRef.current = activeStepIndex;
	}, [activeStepIndex]);

	return (
		<div className="overflow-hidden rounded-lg border transition-[border-color,box-shadow] duration-200 ease-out">
			<div className="border-b bg-muted/25 p-4 transition-colors duration-200">
				<div className="flex flex-col gap-4 lg:flex-row lg:items-center lg:justify-between">
					<div className="space-y-1">
						<p className="text-sm font-semibold">
							{t("settings:settings_mfa_setup_title")}
						</p>
						<p className="text-sm text-muted-foreground">
							{t("settings:settings_mfa_setup_desc")}
						</p>
					</div>
					<SetupStepper activeStep={activeStep} />
				</div>
			</div>

			<SecurityMfaMeasuredMotion className="p-4">
				<SecurityMfaStepMotion activeKey={activeStep} direction={stepDirection}>
					{setupState.step === "intro" ? (
						<SetupIntro
							setupBusy={setupState.busy}
							onCancel={onCancel}
							onContinue={onIntroContinue}
						/>
					) : null}

					{setupState.step === "scan" && setupState.setup ? (
						<SetupScan
							setup={setupState.setup}
							showSecret={setupState.showSecret}
							onBack={onBackToIntro}
							onCancel={onCancel}
							onCopy={onCopy}
							onContinue={onContinueToVerify}
							onToggleSecret={onToggleSecret}
						/>
					) : null}

					{setupState.step === "verify" && setupState.setup ? (
						<SetupVerify
							canFinishSetup={canFinishSetup}
							finishBusy={setupState.finishBusy}
							setupCode={setupState.code}
							setupName={setupState.name}
							onBack={onBackToScan}
							onCancel={onCancel}
							onCodeChange={onCodeChange}
							onFinish={onFinish}
							onNameChange={onNameChange}
						/>
					) : null}

					{setupState.step === "recovery" &&
					setupState.recoveryCodes.length > 0 ? (
						<SetupRecovery
							recoveryCodes={setupState.recoveryCodes}
							recoveryConfirmed={setupState.recoveryConfirmed}
							onConfirmChange={onRecoveryConfirmChange}
							onCopy={onCopyRecoveryCodes}
							onDownload={onDownloadRecoveryCodes}
							onDone={onRecoveryDone}
						/>
					) : null}
				</SecurityMfaStepMotion>
			</SecurityMfaMeasuredMotion>

			{setupState.step !== "recovery" ? (
				<div className="border-t bg-muted/15 px-4 py-3">
					<div className="h-1.5 overflow-hidden rounded-full bg-muted">
						<div
							className="h-full rounded-full bg-primary transition-[width] duration-300 ease-[cubic-bezier(0.22,1,0.36,1)] motion-reduce:transition-none"
							style={{
								width: `${((activeStepIndex + 1) / SETUP_STEPS.length) * 100}%`,
							}}
						/>
					</div>
				</div>
			) : null}
		</div>
	);
}

function SetupStepper({ activeStep }: { activeStep: SetupStep }) {
	const { t } = useTranslation(["settings"]);
	const activeIndex = stepIndex(activeStep);
	return (
		<ol className="grid grid-cols-4 gap-2 text-xs">
			{SETUP_STEPS.map((step, index) => {
				const complete = index < activeIndex;
				const active = step === activeStep;
				return (
					<li
						key={step}
						className={cn(
							"flex min-w-0 items-center gap-2 rounded-md border px-2 py-1.5 transition-[background-color,border-color,color,box-shadow,transform] duration-200 ease-out motion-reduce:transition-none",
							active
								? "border-primary/30 bg-primary/10 text-foreground shadow-xs ring-1 ring-primary/10"
								: complete
									? "border-emerald-200 bg-emerald-50 text-emerald-700 dark:border-emerald-900 dark:bg-emerald-950/40 dark:text-emerald-300"
									: "border-border bg-background text-muted-foreground",
						)}
					>
						<span
							className={cn(
								"flex size-5 shrink-0 items-center justify-center rounded-full border bg-background text-[11px] transition-[background-color,border-color,color,transform] duration-200 ease-out motion-reduce:transition-none",
								active && "scale-105 border-primary/40 text-primary",
								complete && "border-emerald-300 text-emerald-700",
							)}
						>
							{complete ? <Icon name="Check" className="size-3" /> : index + 1}
						</span>
						<span className="truncate">
							{t(`settings:settings_mfa_step_${step}`)}
						</span>
					</li>
				);
			})}
		</ol>
	);
}

function SetupIntro({
	setupBusy,
	onCancel,
	onContinue,
}: {
	setupBusy: boolean;
	onCancel: () => void;
	onContinue: () => void;
}) {
	const { t } = useTranslation(["core", "settings"]);

	return (
		<div className="space-y-4">
			<div className="space-y-1">
				<h4 className="text-base font-semibold">
					{t("settings:settings_mfa_intro_title")}
				</h4>
				<p className="text-sm text-muted-foreground">
					{t("settings:settings_mfa_intro_desc")}
				</p>
			</div>
			<div className="grid gap-3 sm:grid-cols-3">
				<SetupHint
					icon="Shield"
					title={t("settings:settings_mfa_intro_app_title")}
					description={t("settings:settings_mfa_intro_app_desc")}
				/>
				<SetupHint
					icon="Key"
					title={t("settings:settings_mfa_intro_recovery_title")}
					description={t("settings:settings_mfa_intro_recovery_desc")}
				/>
				<SetupHint
					icon="Lock"
					title={t("settings:settings_mfa_intro_password_title")}
					description={t("settings:settings_mfa_intro_password_desc")}
				/>
			</div>
			<div className="flex flex-wrap gap-2">
				<Button type="button" disabled={setupBusy} onClick={onContinue}>
					{setupBusy ? (
						<Icon name="Spinner" className="mr-2 size-4 animate-spin" />
					) : (
						<Icon name="ArrowRight" className="mr-2 size-4" />
					)}
					{t("settings:settings_mfa_intro_continue")}
				</Button>
				<Button
					type="button"
					variant="outline"
					disabled={setupBusy}
					onClick={onCancel}
				>
					{t("core:cancel")}
				</Button>
			</div>
		</div>
	);
}

function SetupHint({
	icon,
	title,
	description,
}: {
	icon: "Shield" | "Key" | "Lock";
	title: string;
	description: string;
}) {
	return (
		<div className="rounded-lg border bg-background p-3 transition-[background-color,border-color,box-shadow,transform] duration-150 ease-out hover:-translate-y-0.5 hover:border-border hover:shadow-xs motion-reduce:transition-none motion-reduce:hover:translate-y-0 dark:hover:shadow-none">
			<Icon name={icon} className="size-5 text-primary" />
			<p className="mt-3 text-sm font-medium">{title}</p>
			<p className="mt-1 text-xs text-muted-foreground">{description}</p>
		</div>
	);
}

function SetupScan({
	setup,
	showSecret,
	onBack,
	onCancel,
	onCopy,
	onContinue,
	onToggleSecret,
}: {
	setup: TotpSetupStartResponse;
	showSecret: boolean;
	onBack: () => void;
	onCancel: () => void;
	onCopy: (value: string, message?: string) => void;
	onContinue: () => void;
	onToggleSecret: () => void;
}) {
	const { t } = useTranslation(["core", "settings"]);
	const maskedSecret = "••••••••••••••••";
	return (
		<div className="grid gap-5 lg:grid-cols-[240px_minmax(0,1fr)]">
			<SetupQrCode otpauthUri={setup.otpauth_uri} />
			<div className="min-w-0 space-y-4">
				<div className="space-y-1">
					<h4 className="text-base font-semibold">
						{t("settings:settings_mfa_scan_title")}
					</h4>
					<p className="text-sm text-muted-foreground">
						{t("settings:settings_mfa_scan_desc")}
					</p>
				</div>
				<div className="space-y-2">
					<Label htmlFor="mfa-setup-secret">
						{t("settings:settings_mfa_secret")}
					</Label>
					<div className="grid gap-2 md:grid-cols-[minmax(0,1fr)_auto]">
						<Input
							id="mfa-setup-secret"
							readOnly
							value={showSecret ? setup.secret : maskedSecret}
							className={cn("font-mono", showSecret ? "" : "select-none")}
						/>
						<div className="flex gap-2">
							<Button type="button" variant="outline" onClick={onToggleSecret}>
								<Icon
									name={showSecret ? "EyeSlash" : "Eye"}
									className="mr-2 size-4"
								/>
								{showSecret
									? t("settings:settings_mfa_hide_secret")
									: t("settings:settings_mfa_show_secret")}
							</Button>
							<Button
								type="button"
								variant="outline"
								onClick={() =>
									onCopy(setup.secret, t("settings:settings_mfa_secret_copied"))
								}
							>
								<Icon name="Copy" className="mr-2 size-4" />
								{t("core:copy")}
							</Button>
						</div>
					</div>
				</div>
				<div className="flex flex-wrap gap-2">
					<Button type="button" onClick={onContinue}>
						<Icon name="ArrowRight" className="mr-2 size-4" />
						{t("settings:settings_mfa_scan_continue")}
					</Button>
					<Button type="button" variant="outline" onClick={onBack}>
						<Icon name="ArrowLeft" className="mr-2 size-4" />
						{t("core:back")}
					</Button>
					<Button type="button" variant="ghost" onClick={onCancel}>
						{t("core:cancel")}
					</Button>
				</div>
			</div>
		</div>
	);
}

function SetupQrCode({ otpauthUri }: { otpauthUri: string }) {
	const { t } = useTranslation(["core", "settings"]);
	const qr = useMemo(() => {
		try {
			return {
				error: "",
				symbol: QRCode.create(otpauthUri, {
					errorCorrectionLevel: "M",
				}),
			};
		} catch (error) {
			return {
				error: error instanceof Error ? error.message : String(error),
				symbol: null,
			};
		}
	}, [otpauthUri]);
	const modules = qr.symbol?.modules;
	const margin = 1;
	const size = modules ? modules.size + margin * 2 : 0;

	return (
		<div className="flex items-center justify-center rounded-lg border bg-white p-4">
			{modules ? (
				<svg
					viewBox={`0 0 ${size} ${size}`}
					role="img"
					aria-label={t("settings:settings_mfa_qr_alt")}
					className="size-48 text-gray-950"
					shapeRendering="crispEdges"
				>
					<rect width={size} height={size} fill="white" />
					<path d={qrPath(modules, margin)} fill="currentColor" />
				</svg>
			) : (
				<div className="flex size-48 items-center justify-center rounded-md border border-dashed text-center text-xs text-muted-foreground">
					{qr.error || t("core:loading")}
				</div>
			)}
		</div>
	);
}

function qrPath(modules: QrModules, margin: number) {
	const parts: string[] = [];
	for (let row = 0; row < modules.size; row += 1) {
		for (let col = 0; col < modules.size; col += 1) {
			if (modules.get(row, col)) {
				parts.push(`M${col + margin} ${row + margin}h1v1H${col + margin}z`);
			}
		}
	}
	return parts.join("");
}

function SetupVerify({
	canFinishSetup,
	finishBusy,
	setupCode,
	setupName,
	onBack,
	onCancel,
	onCodeChange,
	onFinish,
	onNameChange,
}: {
	canFinishSetup: boolean;
	finishBusy: boolean;
	setupCode: string;
	setupName: string;
	onBack: () => void;
	onCancel: () => void;
	onCodeChange: (value: string) => void;
	onFinish: () => void;
	onNameChange: (value: string) => void;
}) {
	const { t } = useTranslation(["core", "settings"]);

	return (
		<div className="max-w-2xl space-y-4">
			<div className="space-y-1">
				<h4 className="text-base font-semibold">
					{t("settings:settings_mfa_verify_title")}
				</h4>
				<p className="text-sm text-muted-foreground">
					{t("settings:settings_mfa_verify_desc")}
				</p>
			</div>
			<div className="grid gap-3 md:grid-cols-2">
				<div className="space-y-2">
					<Label htmlFor="mfa-setup-name">
						{t("settings:settings_mfa_factor_name")}
					</Label>
					<Input
						id="mfa-setup-name"
						value={setupName}
						placeholder={t("settings:settings_mfa_factor_name_placeholder")}
						onChange={(event) => onNameChange(event.target.value)}
					/>
				</div>
				<div className="space-y-2">
					<Label htmlFor="mfa-setup-code">
						{t("settings:settings_mfa_totp_code")}
					</Label>
					<Input
						id="mfa-setup-code"
						value={setupCode}
						inputMode="numeric"
						autoComplete="one-time-code"
						placeholder="123456"
						onChange={(event) => onCodeChange(event.target.value)}
					/>
				</div>
			</div>
			<div className="flex flex-wrap gap-2">
				<Button type="button" disabled={!canFinishSetup} onClick={onFinish}>
					{finishBusy ? (
						<Icon name="Spinner" className="mr-2 size-4 animate-spin" />
					) : (
						<Icon name="Check" className="mr-2 size-4" />
					)}
					{t("settings:settings_mfa_finish_setup")}
				</Button>
				<Button
					type="button"
					variant="outline"
					disabled={finishBusy}
					onClick={onBack}
				>
					<Icon name="ArrowLeft" className="mr-2 size-4" />
					{t("core:back")}
				</Button>
				<Button
					type="button"
					variant="ghost"
					disabled={finishBusy}
					onClick={onCancel}
				>
					{t("core:cancel")}
				</Button>
			</div>
		</div>
	);
}

function SetupRecovery({
	recoveryCodes,
	recoveryConfirmed,
	onConfirmChange,
	onCopy,
	onDownload,
	onDone,
}: {
	recoveryCodes: string[];
	recoveryConfirmed: boolean;
	onConfirmChange: () => void;
	onCopy: () => void;
	onDownload: () => void;
	onDone: () => void;
}) {
	const { t } = useTranslation(["settings"]);

	return (
		<div className="space-y-4">
			<div className="space-y-1">
				<h4 className="text-base font-semibold">
					{t("settings:settings_mfa_recovery_codes_title")}
				</h4>
				<p className="text-sm text-muted-foreground">
					{t("settings:settings_mfa_recovery_codes_desc")}
				</p>
			</div>
			<div className="grid gap-2 sm:grid-cols-2 lg:grid-cols-3">
				{recoveryCodes.map((code, index) => (
					<code
						key={code}
						className="animate-in fade-in slide-in-from-bottom-1 rounded-md border bg-background px-3 py-2 text-sm duration-200 motion-reduce:animate-none"
						style={{ animationDelay: `${index * 24}ms` }}
					>
						{code}
					</code>
				))}
			</div>
			<div className="flex flex-wrap gap-2">
				<Button type="button" onClick={onDownload}>
					<Icon name="Download" className="mr-2 size-4" />
					{t("settings:settings_mfa_download_recovery_codes")}
				</Button>
				<Button type="button" variant="outline" onClick={onCopy}>
					<Icon name="Copy" className="mr-2 size-4" />
					{t("settings:settings_mfa_copy_recovery_codes")}
				</Button>
			</div>
			<button
				type="button"
				className="flex w-full items-start gap-3 rounded-lg border bg-muted/20 p-3 text-left text-sm transition-colors hover:bg-muted/35"
				aria-pressed={recoveryConfirmed}
				onClick={onConfirmChange}
			>
				<span
					className={cn(
						"mt-0.5 inline-flex size-5 shrink-0 items-center justify-center rounded-md border shadow-sm transition-colors dark:shadow-none",
						recoveryConfirmed
							? "border-primary bg-primary text-primary-foreground"
							: "border-muted-foreground/70 bg-background text-transparent",
					)}
				>
					<Icon name="Check" className="size-3.5" />
				</span>
				<span>
					<span className="font-medium">
						{t("settings:settings_mfa_recovery_confirm_title")}
					</span>
					<span className="mt-1 block text-muted-foreground">
						{t("settings:settings_mfa_recovery_confirm_desc")}
					</span>
				</span>
			</button>
			<Button type="button" disabled={!recoveryConfirmed} onClick={onDone}>
				<Icon name="Check" className="mr-2 size-4" />
				{t("settings:settings_mfa_done")}
			</Button>
		</div>
	);
}
