import { useTranslation } from "react-i18next";
import { Badge } from "@/components/ui/badge";
import { Button } from "@/components/ui/button";
import { Icon } from "@/components/ui/icon";
import { ADMIN_CONTROL_HEIGHT_CLASS } from "@/lib/constants";
import type { RemoteEnrollmentCommandInfo } from "@/types/api";

interface RemoteNodeEnrollmentPanelProps {
	command: RemoteEnrollmentCommandInfo | null;
	errorMessage?: string | null;
	loading?: boolean;
	onGenerate?: () => void;
	onCopy: (value: string) => Promise<void>;
	onRetry?: () => void;
}

export function RemoteNodeEnrollmentPanel({
	command,
	errorMessage,
	loading = false,
	onCopy,
	onGenerate,
	onRetry,
}: RemoteNodeEnrollmentPanelProps) {
	const { t } = useTranslation("admin");
	return (
		<section className="animate-in fade-in slide-in-from-top-2 space-y-6 duration-300 motion-reduce:animate-none">
			<div className="rounded-xl bg-primary/10 p-5">
				<div className="flex flex-wrap items-start justify-between gap-3">
					<div>
						<p className="text-[11px] font-medium uppercase tracking-[0.2em] text-primary">
							{t("remote_node_enrollment_phase_label")}
						</p>
						<h2 className="mt-1 text-xl font-semibold">
							{t("remote_node_enrollment_panel_title")}
						</h2>
						<p className="mt-2 max-w-3xl text-sm leading-6 text-muted-foreground">
							{t("remote_node_enrollment_panel_desc")}
						</p>
					</div>
					<Badge
						variant="outline"
						className="border-primary/30 bg-background/70"
					>
						{errorMessage
							? t("remote_node_enrollment_command_failed_short")
							: loading
								? t("remote_node_enrollment_command_generating")
								: command
									? t("remote_node_enrollment_command_ready")
									: t("remote_node_enrollment_command_pending")}
					</Badge>
				</div>
			</div>

			{errorMessage ? (
				<div className="rounded-xl bg-destructive/10 p-4 text-sm text-destructive">
					<div className="flex items-start gap-3">
						<Icon name="Warning" className="mt-0.5 size-4 shrink-0" />
						<div className="min-w-0 flex-1">
							<p className="font-medium">
								{t("remote_node_enrollment_command_failed")}
							</p>
							<p className="mt-1 break-words text-xs">{errorMessage}</p>
							{onRetry ? (
								<Button
									type="button"
									variant="outline"
									className={`${ADMIN_CONTROL_HEIGHT_CLASS} mt-3`}
									onClick={onRetry}
								>
									<Icon name="ArrowsClockwise" className="mr-1 size-4" />
									{t("remote_node_enrollment_retry")}
								</Button>
							) : null}
						</div>
					</div>
				</div>
			) : loading ? (
				<div className="flex min-h-48 items-center justify-center gap-3 text-sm text-muted-foreground">
					<Icon name="Spinner" className="size-4 animate-spin" />
					{t("remote_node_enrollment_command_generating")}
				</div>
			) : command ? (
				<section className="rounded-xl bg-muted/30 p-5">
					<div className="flex flex-col gap-3 sm:flex-row sm:items-start sm:justify-between">
						<div className="min-w-0">
							<h3 className="text-base font-semibold">
								{t("remote_node_enrollment_command_title")}
							</h3>
							<p className="mt-1 text-sm text-muted-foreground">
								{t("remote_node_enrollment_command_desc")}
							</p>
						</div>
						<Button
							type="button"
							variant="outline"
							className={ADMIN_CONTROL_HEIGHT_CLASS}
							onClick={() => void onCopy(command.command)}
						>
							<Icon name="Copy" className="mr-1 size-4" />
							{t("remote_node_enrollment_copy_command")}
						</Button>
					</div>
					<pre className="max-h-52 overflow-x-auto whitespace-pre rounded-lg border border-border/70 bg-background p-4 font-mono text-xs leading-6 shadow-inner">
						{command.command}
					</pre>
					<p className="text-xs leading-5 text-muted-foreground">
						{t("remote_node_enrollment_command_hint")}
					</p>
				</section>
			) : (
				<div className="rounded-xl bg-muted/30 p-5 text-sm text-muted-foreground">
					<p>{t("remote_node_enrollment_command_pending_desc")}</p>
					{onGenerate ? (
						<Button
							type="button"
							className={`${ADMIN_CONTROL_HEIGHT_CLASS} mt-4`}
							onClick={onGenerate}
						>
							<Icon name="Plus" className="mr-1 size-4" />
							{t("remote_node_enrollment_generate")}
						</Button>
					) : null}
				</div>
			)}
		</section>
	);
}
