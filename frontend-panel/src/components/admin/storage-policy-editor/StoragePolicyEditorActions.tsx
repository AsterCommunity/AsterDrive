import { useTranslation } from "react-i18next";
import { Button } from "@/components/ui/button";
import { Icon } from "@/components/ui/icon";
import { ADMIN_CONTROL_HEIGHT_CLASS } from "@/lib/constants";
import type { StorageConnectorDescriptor } from "@/types/api";
import {
	supportsDraftConnectionTest,
	supportsSavedConnectionTest,
} from "./descriptorPredicates";
import { StoragePolicyTestConnectionButton } from "./StoragePolicyTestConnectionButton";

const CREATE_LAST_STEP = 2;

interface StoragePolicyEditorActionsProps {
	mode: "create" | "edit";
	createStep: number;
	submitting: boolean;
	descriptor: StorageConnectorDescriptor | null;
	onBack: () => void;
	onCancel?: () => void;
	onRunConnectionTest: () => Promise<boolean>;
}

/**
 * 编辑器头部操作组：返回/取消 + 连接测试 + 主提交按钮。
 * 由 admin 编辑页页头与 setup 向导壳共用；主按钮 type="submit"，
 * 依赖外层 <form> 的 onSubmit 驱动提交。
 */
export function StoragePolicyEditorActions({
	mode,
	createStep,
	submitting,
	descriptor,
	onBack,
	onCancel,
	onRunConnectionTest,
}: StoragePolicyEditorActionsProps) {
	const { t } = useTranslation("admin");
	const isCreateMode = mode === "create";
	const canDraftTest = supportsDraftConnectionTest(descriptor);
	const canRunConnectionTest = isCreateMode
		? canDraftTest
		: canDraftTest || supportsSavedConnectionTest(descriptor);
	const showTest = canRunConnectionTest && (!isCreateMode || createStep > 0);
	const showPrimary = !isCreateMode || createStep > 0;
	const primaryLabel = isCreateMode
		? createStep === CREATE_LAST_STEP
			? t("core:create")
			: createStep === CREATE_LAST_STEP - 1
				? t("policy_wizard_review")
				: t("policy_wizard_next")
		: t("save_changes");

	return (
		<>
			{isCreateMode && createStep > 0 ? (
				<Button
					type="button"
					variant="outline"
					size="sm"
					className={ADMIN_CONTROL_HEIGHT_CLASS}
					disabled={submitting}
					onClick={onBack}
				>
					{t("core:back")}
				</Button>
			) : onCancel ? (
				<Button
					type="button"
					variant="outline"
					size="sm"
					className={ADMIN_CONTROL_HEIGHT_CLASS}
					disabled={submitting}
					onClick={onCancel}
				>
					{t("core:cancel")}
				</Button>
			) : null}
			{showTest ? (
				<StoragePolicyTestConnectionButton
					onTest={onRunConnectionTest}
					disabled={submitting}
				/>
			) : null}
			{showPrimary ? (
				<Button
					type="submit"
					size="sm"
					className={ADMIN_CONTROL_HEIGHT_CLASS}
					disabled={submitting || !descriptor}
				>
					{submitting ? (
						<Icon name="Spinner" className="mr-1 size-4 animate-spin" />
					) : null}
					{primaryLabel}
				</Button>
			) : null}
		</>
	);
}
