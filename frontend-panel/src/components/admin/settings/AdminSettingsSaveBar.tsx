import type { RefObject } from "react";
import { useRef } from "react";
import { useTranslation } from "react-i18next";
import { ADMIN_SETTINGS_CONTENT_MAX_WIDTH_CLASS } from "@/components/admin/settings/adminSettingsAnimation";
import { Button } from "@/components/ui/button";
import { cn } from "@/lib/utils";

type SaveBarPhase = "hidden" | "entering" | "visible" | "exiting";

interface AdminSettingsSaveBarProps {
	changedCount: number;
	hasUnsavedChanges: boolean;
	hasValidationError: boolean;
	measureRef: RefObject<HTMLDivElement | null>;
	phase: SaveBarPhase;
	saving: boolean;
	transitionDurationMs: number;
	validationMessage?: string;
	onDiscardChanges: () => void;
	onSaveAll: () => void;
}

export function AdminSettingsSaveBar({
	changedCount,
	hasUnsavedChanges,
	hasValidationError,
	measureRef,
	phase,
	saving,
	transitionDurationMs,
	validationMessage,
	onDiscardChanges,
	onSaveAll,
}: AdminSettingsSaveBarProps) {
	const { t } = useTranslation("admin");
	const latestVisibleStateRef = useRef({
		changedCount,
		hasValidationError,
		saving,
		validationMessage,
	});

	if (phase === "hidden") {
		return null;
	}

	if (hasUnsavedChanges) {
		latestVisibleStateRef.current = {
			changedCount,
			hasValidationError,
			saving,
			validationMessage,
		};
	}

	const displayState =
		phase === "exiting"
			? latestVisibleStateRef.current
			: {
					changedCount,
					hasValidationError,
					saving,
					validationMessage,
				};
	const actionsDisabled = phase === "exiting" || saving || !hasUnsavedChanges;

	return (
		<div
			ref={measureRef}
			data-testid="settings-save-bar"
			aria-hidden={!hasUnsavedChanges || phase === "exiting"}
			className="pointer-events-none fixed right-0 bottom-0 left-0 z-(--z-fixed) px-4 pb-4 md:left-60 md:px-6 md:pb-6"
		>
			<div
				className={cn(
					"mx-auto w-full origin-bottom transition-[opacity,transform] will-change-transform motion-reduce:transition-none",
					ADMIN_SETTINGS_CONTENT_MAX_WIDTH_CLASS,
					phase === "entering"
						? "pointer-events-none translate-y-2 opacity-0 ease-out"
						: phase === "visible"
							? "pointer-events-auto translate-y-0 scale-100 opacity-100 ease-out"
							: "pointer-events-none translate-y-0 scale-100 opacity-0 ease-out",
				)}
				style={{ transitionDuration: `${transitionDurationMs}ms` }}
			>
				<div
					className={cn(
						"rounded-lg border bg-card/95 shadow-lg shadow-black/5 ring-1 backdrop-blur supports-[backdrop-filter]:bg-card/90 dark:shadow-none",
						displayState.hasValidationError
							? "border-destructive/40 ring-destructive/10"
							: "border-border/70 ring-border/50",
					)}
				>
					<div className="flex flex-col gap-3 px-4 py-3 sm:flex-row sm:items-center sm:justify-between sm:px-5">
						<div className="min-w-0 flex-1 space-y-1">
							<p className="text-sm font-medium">
								{t("settings_save_notice", {
									count: displayState.changedCount,
								})}
							</p>
							<p
								className={
									displayState.hasValidationError
										? "text-sm text-destructive"
										: "text-sm text-muted-foreground"
								}
							>
								{displayState.hasValidationError
									? (displayState.validationMessage ??
										t("custom_config_validation_error"))
									: t("settings_save_hint")}
							</p>
						</div>
						<div className="flex flex-wrap items-center gap-3 sm:justify-end">
							<Button
								variant="ghost"
								disabled={actionsDisabled}
								onClick={onDiscardChanges}
							>
								{t("undo_changes")}
							</Button>
							<Button
								className="w-fit"
								disabled={actionsDisabled || displayState.hasValidationError}
								onClick={onSaveAll}
							>
								{t("save_changes")}
							</Button>
						</div>
					</div>
				</div>
			</div>
		</div>
	);
}
