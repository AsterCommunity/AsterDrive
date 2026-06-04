import { useTranslation } from "react-i18next";
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
import { Label } from "@/components/ui/label";
import {
	Select,
	SelectContent,
	SelectItem,
	SelectTrigger,
	SelectValue,
} from "@/components/ui/select";
import { ADMIN_CONTROL_HEIGHT_CLASS } from "@/lib/constants";

interface PolicyGroupMigrationDialogProps {
	open: boolean;
	error: string | null;
	loading: boolean;
	sourceGroupName: string | null;
	submitting: boolean;
	targetGroupId: string;
	targetOptions: Array<{
		label: string;
		value: string;
	}>;
	onConfirm: () => void;
	onOpenChange: (open: boolean) => void;
	onTargetGroupChange: (value: string) => void;
}

export function PolicyGroupMigrationDialog({
	open,
	error,
	loading,
	sourceGroupName,
	submitting,
	targetGroupId,
	targetOptions,
	onConfirm,
	onOpenChange,
	onTargetGroupChange,
}: PolicyGroupMigrationDialogProps) {
	const { t } = useTranslation("admin");
	const targetSelectionDisabled =
		loading || submitting || targetOptions.length === 0;

	return (
		<Dialog open={open} onOpenChange={onOpenChange}>
			<DialogContent className="sm:max-w-lg">
				<DialogHeader>
					<DialogTitle>{t("migrate_policy_group_assignments")}</DialogTitle>
					<DialogDescription>
						{t("policy_group_migration_desc")}
					</DialogDescription>
				</DialogHeader>
				<div className="space-y-4">
					<div className="grid gap-4 sm:grid-cols-2">
						<div className="space-y-2">
							<Label>{t("policy_group_migration_source")}</Label>
							<div className="rounded-lg border bg-muted/20 px-3 py-2 text-sm font-medium text-foreground">
								{sourceGroupName ?? "-"}
							</div>
						</div>
						<div className="space-y-2">
							<Label>{t("policy_group_migration_target")}</Label>
							<Select
								items={targetOptions}
								value={targetGroupId}
								onValueChange={(value) => onTargetGroupChange(value ?? "")}
								disabled={targetSelectionDisabled}
							>
								<SelectTrigger
									className={`${ADMIN_CONTROL_HEIGHT_CLASS} w-full`}
								>
									<SelectValue
										placeholder={t("policy_group_migration_target_placeholder")}
									/>
								</SelectTrigger>
								<SelectContent>
									{targetOptions.map((group) => (
										<SelectItem key={group.value} value={group.value}>
											{group.label}
										</SelectItem>
									))}
								</SelectContent>
							</Select>
						</div>
					</div>

					<div className="rounded-lg border bg-muted/20 px-3 py-2 text-sm text-muted-foreground">
						{t("policy_group_migration_hint")}
					</div>

					{error ? (
						<div className="rounded-lg border border-destructive/30 bg-destructive/5 px-3 py-2 text-sm text-destructive">
							{error}
						</div>
					) : null}
				</div>
				<DialogFooter className="gap-2">
					<Button
						type="button"
						variant="outline"
						onClick={() => onOpenChange(false)}
					>
						{t("core:cancel")}
					</Button>
					<Button
						type="button"
						onClick={onConfirm}
						disabled={targetSelectionDisabled}
					>
						{submitting ? (
							<Icon name="Spinner" className="mr-1 size-4 animate-spin" />
						) : (
							<Icon name="ArrowsClockwise" className="mr-1 size-4" />
						)}
						{t("policy_group_migration_confirm")}
					</Button>
				</DialogFooter>
			</DialogContent>
		</Dialog>
	);
}
