import type { FormEvent } from "react";
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
import { DialogLifecycle } from "@/components/ui/dialog-lifecycle";
import { Icon } from "@/components/ui/icon";
import { Input } from "@/components/ui/input";
import { Label } from "@/components/ui/label";
import {
	Select,
	SelectContent,
	SelectItem,
	SelectTrigger,
	SelectValue,
} from "@/components/ui/select";
import { ADMIN_CONTROL_HEIGHT_CLASS } from "@/lib/constants";

interface AdminTaskCleanupDialogProps {
	description: string;
	finishedBefore: string;
	kindFilter: string;
	kindOptions: ReadonlyArray<{ label: string; value: string }>;
	onFinishedBeforeChange: (value: string) => void;
	onKindFilterChange: (value: string | null) => void;
	onOpenChange: (open: boolean) => void;
	onResetConditions: () => void;
	onStatusFilterChange: (value: string | null) => void;
	onSubmit: (event: FormEvent<HTMLFormElement>) => void;
	open: boolean;
	statusFilter: string;
	statusOptions: ReadonlyArray<{ label: string; value: string }>;
	submitDisabled: boolean;
	submitting: boolean;
}

export function AdminTaskCleanupDialog(props: AdminTaskCleanupDialogProps) {
	return (
		<DialogLifecycle
			open={props.open}
			target={props}
			onOpenChange={props.onOpenChange}
		>
			{({ target, active, onOpenChange }) => (
				<AdminTaskCleanupDialogContent
					{...target}
					open={active}
					onOpenChange={onOpenChange}
				/>
			)}
		</DialogLifecycle>
	);
}

function AdminTaskCleanupDialogContent({
	description,
	finishedBefore,
	kindFilter,
	kindOptions,
	onFinishedBeforeChange,
	onKindFilterChange,
	onOpenChange,
	onResetConditions,
	onStatusFilterChange,
	onSubmit,
	open,
	statusFilter,
	statusOptions,
	submitDisabled,
	submitting,
}: AdminTaskCleanupDialogProps) {
	const { t } = useTranslation(["admin", "core"]);

	return (
		<Dialog open={open} onOpenChange={onOpenChange}>
			<DialogContent keepMounted className="sm:max-w-md">
				<form onSubmit={onSubmit} className="space-y-4">
					<DialogHeader>
						<DialogTitle>{t("admin:task_cleanup_title")}</DialogTitle>
						<DialogDescription>
							{t("admin:task_cleanup_desc")}
						</DialogDescription>
					</DialogHeader>
					<div className="space-y-2">
						<Label htmlFor="task-cleanup-finished-before">
							{t("admin:task_cleanup_finished_before")}
						</Label>
						<Input
							id="task-cleanup-finished-before"
							type="datetime-local"
							value={finishedBefore}
							onChange={(event) => onFinishedBeforeChange(event.target.value)}
							aria-label={t("admin:task_cleanup_finished_before")}
							className={ADMIN_CONTROL_HEIGHT_CLASS}
						/>
					</div>
					<div className="space-y-2">
						<Label htmlFor="task-cleanup-kind">
							{t("admin:all_task_types")}
						</Label>
						<Select
							items={kindOptions}
							value={kindFilter}
							onValueChange={onKindFilterChange}
						>
							<SelectTrigger id="task-cleanup-kind" width="full">
								<SelectValue />
							</SelectTrigger>
							<SelectContent>
								{kindOptions.map((option) => (
									<SelectItem key={option.value} value={option.value}>
										{option.label}
									</SelectItem>
								))}
							</SelectContent>
						</Select>
					</div>
					<div className="space-y-2">
						<Label htmlFor="task-cleanup-status">
							{t("admin:all_completed_task_statuses")}
						</Label>
						<Select
							items={statusOptions}
							value={statusFilter}
							onValueChange={onStatusFilterChange}
						>
							<SelectTrigger id="task-cleanup-status" width="full">
								<SelectValue />
							</SelectTrigger>
							<SelectContent>
								{statusOptions.map((option) => (
									<SelectItem key={option.value} value={option.value}>
										{option.label}
									</SelectItem>
								))}
							</SelectContent>
						</Select>
					</div>
					<div className="rounded-lg border bg-muted/20 px-3 py-2 text-xs text-muted-foreground">
						{description}
					</div>
					<DialogFooter>
						<Button
							type="button"
							variant="outline"
							onClick={() => onOpenChange(false)}
							disabled={submitting}
						>
							{t("core:cancel")}
						</Button>
						<Button
							type="button"
							variant="ghost"
							onClick={onResetConditions}
							disabled={submitting}
						>
							{t("admin:reset_cleanup_conditions")}
						</Button>
						<Button
							type="submit"
							variant="destructive"
							disabled={submitDisabled}
						>
							{submitting ? (
								<Icon name="Spinner" className="mr-1 size-4 animate-spin" />
							) : (
								<Icon name="Trash" className="mr-1 size-4" />
							)}
							{t("admin:cleanup_completed_tasks")}
						</Button>
					</DialogFooter>
				</form>
			</DialogContent>
		</Dialog>
	);
}
