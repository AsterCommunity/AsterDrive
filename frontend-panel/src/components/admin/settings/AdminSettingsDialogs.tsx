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
import { Input } from "@/components/ui/input";
import type { TemplateVariableGroup } from "@/types/api";

interface MailTemplateVariablesDialogProps {
	activeGroup: TemplateVariableGroup | null;
	activeGroupCode: string | null;
	getVariableDescription: (
		variable: TemplateVariableGroup["variables"][number],
	) => string | undefined;
	getVariableGroupLabel: (group: TemplateVariableGroup) => string;
	getVariableLabel: (
		variable: TemplateVariableGroup["variables"][number],
	) => string;
	onOpenChange: (open: boolean) => void;
}

export function MailTemplateVariablesDialog(
	props: MailTemplateVariablesDialogProps,
) {
	return (
		<DialogLifecycle
			open={props.activeGroupCode !== null}
			target={props}
			onOpenChange={props.onOpenChange}
		>
			{({ target, active, onOpenChange }) => (
				<MailTemplateVariablesContent
					{...target}
					open={active}
					onOpenChange={onOpenChange}
				/>
			)}
		</DialogLifecycle>
	);
}

function MailTemplateVariablesContent({
	open,
	activeGroup,
	getVariableDescription,
	getVariableGroupLabel,
	getVariableLabel,
	onOpenChange,
}: MailTemplateVariablesDialogProps & { open: boolean }) {
	const { t } = useTranslation("admin");

	return (
		<Dialog open={open} onOpenChange={onOpenChange}>
			<DialogContent className="max-w-[calc(100%-1.5rem)] sm:max-w-[min(72rem,calc(100vw-2rem))]">
				<DialogHeader>
					<DialogTitle>
						{t("mail_template_variables_dialog_title", {
							name: activeGroup ? getVariableGroupLabel(activeGroup) : "",
						})}
					</DialogTitle>
					<DialogDescription>
						{t("mail_template_variables_dialog_desc")}
					</DialogDescription>
				</DialogHeader>
				<div className="max-h-[min(70vh,40rem)] space-y-4 overflow-y-auto py-2 pr-1">
					{activeGroup ? (
						<div className="space-y-3">
							{activeGroup.variables.map((variable) => (
								<div
									key={`${activeGroup.template_code}:${variable.token}`}
									className="rounded-xl border border-border/60 bg-card/40 p-4"
								>
									<div className="flex flex-wrap items-center gap-2">
										<code className="break-all rounded bg-muted px-2 py-1 font-mono text-xs">
											{variable.token}
										</code>
										<span className="text-sm font-medium">
											{getVariableLabel(variable)}
										</span>
									</div>
									{getVariableDescription(variable) ? (
										<p className="mt-2 break-words text-sm leading-6 text-muted-foreground">
											{getVariableDescription(variable)}
										</p>
									) : null}
								</div>
							))}
						</div>
					) : (
						<p className="text-sm text-muted-foreground">
							{t("mail_template_variables_dialog_empty")}
						</p>
					)}
				</div>
				<DialogFooter showCloseButton />
			</DialogContent>
		</Dialog>
	);
}

interface TestEmailDialogProps {
	open: boolean;
	sending: boolean;
	target: string;
	onOpenChange: (open: boolean) => void;
	onSend: () => void;
	onTargetChange: (value: string) => void;
}

export function TestEmailDialog(props: TestEmailDialogProps) {
	return (
		<DialogLifecycle
			open={props.open}
			target={props}
			onOpenChange={props.onOpenChange}
		>
			{({ target, active, onOpenChange }) => (
				<TestEmailDialogContent
					{...target}
					open={active}
					onOpenChange={onOpenChange}
				/>
			)}
		</DialogLifecycle>
	);
}

function TestEmailDialogContent({
	open,
	sending,
	target,
	onOpenChange,
	onSend,
	onTargetChange,
}: TestEmailDialogProps) {
	const { t } = useTranslation("admin");

	return (
		<Dialog open={open} onOpenChange={onOpenChange}>
			<DialogContent className="max-w-md">
				<DialogHeader>
					<DialogTitle>{t("mail_test_email_dialog_title")}</DialogTitle>
					<DialogDescription>
						{t("mail_test_email_dialog_desc")}
					</DialogDescription>
				</DialogHeader>
				<div className="space-y-2 py-2">
					<p className="text-sm font-medium">
						{t("mail_test_email_recipient_label")}
					</p>
					<Input
						type="email"
						value={target}
						onChange={(event) => onTargetChange(event.target.value)}
						placeholder={t("mail_test_email_recipient_placeholder")}
					/>
				</div>
				<DialogFooter>
					<Button
						variant="outline"
						disabled={sending}
						onClick={() => onOpenChange(false)}
					>
						{t("core:cancel")}
					</Button>
					<Button disabled={sending} onClick={onSend}>
						{t("mail_send_test_email")}
					</Button>
				</DialogFooter>
			</DialogContent>
		</Dialog>
	);
}
