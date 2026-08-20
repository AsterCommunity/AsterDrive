import { useTranslation } from "react-i18next";
import { useNavigate } from "react-router-dom";
import { Badge } from "@/components/ui/badge";
import { Button } from "@/components/ui/button";
import { Icon } from "@/components/ui/icon";
import { ADMIN_CONTROL_HEIGHT_CLASS } from "@/lib/constants";
import { formatDateAbsolute } from "@/lib/format";
import { cn } from "@/lib/utils";
import type { AdminSystemHealthSummary } from "@/types/api";

function systemHealthPresentation(status: AdminSystemHealthSummary["status"]) {
	// D9 去框化：banner 去 border 留状态色底（色即状态，非装饰）
	switch (status) {
		case "healthy":
			return {
				icon: "Check" as const,
				labelKey: "overview_system_health_healthy",
				className:
					"bg-emerald-50 text-emerald-950 dark:bg-emerald-950/40 dark:text-emerald-100",
				iconClass:
					"bg-emerald-100 text-emerald-700 dark:bg-emerald-900 dark:text-emerald-200",
			};
		case "degraded":
			return {
				icon: "Warning" as const,
				labelKey: "overview_system_health_degraded",
				className:
					"bg-amber-50 text-amber-950 dark:bg-amber-950/40 dark:text-amber-100",
				iconClass:
					"bg-amber-100 text-amber-700 dark:bg-amber-900 dark:text-amber-200",
			};
		case "unhealthy":
			return {
				icon: "CircleAlert" as const,
				labelKey: "overview_system_health_unhealthy",
				className:
					"bg-red-50 text-red-950 dark:bg-red-950/40 dark:text-red-100",
				iconClass: "bg-red-100 text-red-700 dark:bg-red-900 dark:text-red-200",
			};
		case "unknown":
			return {
				icon: "Info" as const,
				labelKey: "overview_system_health_unknown",
				className: "bg-muted/30 text-foreground",
				iconClass: "bg-muted text-muted-foreground",
			};
	}
}

export function SystemHealthBanner({
	health,
}: {
	health: AdminSystemHealthSummary;
}) {
	const { t } = useTranslation("admin");
	const navigate = useNavigate();
	const presentation = systemHealthPresentation(health.status);
	const formatComponentName = (name: string) =>
		t(`overview_system_health_component_${name}`, {
			defaultValue: name,
		});
	const formatComponentStatus = (status: AdminSystemHealthSummary["status"]) =>
		t(`overview_system_health_status_${status}`, {
			defaultValue: status,
		});
	const formatIssueComponent = (
		component: AdminSystemHealthSummary["components"][number],
	) => {
		const componentName = formatComponentName(component.name);
		const status = formatComponentStatus(component.status);
		return t("overview_system_health_issue_component", {
			component: componentName,
			defaultValue: `${componentName}: ${status}`,
			status,
		});
	};
	const checkedAt = health.checked_at
		? t("overview_system_health_checked_at", {
				date: formatDateAbsolute(health.checked_at),
			})
		: t("overview_system_health_not_checked");
	const isIssue = health.status === "degraded" || health.status === "unhealthy";
	const issueComponents = health.components.filter(
		(component) => component.status !== "healthy",
	);
	const issueSummary =
		issueComponents.length > 0
			? t("overview_system_health_issue_summary", {
					components: issueComponents
						.map((component) => formatComponentName(component.name))
						.join(
							t("overview_system_health_issue_separator", {
								defaultValue: ", ",
							}),
						),
				})
			: null;
	const message = isIssue
		? (issueSummary ?? health.summary ?? t("overview_system_health_no_summary"))
		: health.status === "healthy"
			? t("overview_system_health_healthy_desc")
			: t("overview_system_health_unknown_desc");

	const historyButton = health.task_id ? (
		<Button
			variant="outline"
			size="sm"
			className={cn(
				ADMIN_CONTROL_HEIGHT_CLASS,
				"shrink-0",
				isIssue && "border-current/25 bg-background/40 hover:bg-background/70",
			)}
			onClick={() => {
				void navigate("/admin/tasks?kind=system_runtime");
			}}
		>
			<Icon name="ArrowSquareOut" className="size-4" />
			{t("overview_system_health_view_history")}
		</Button>
	) : null;

	// 正常/未知态收敛为单行状态：「没事」不占用全页最响的色块，异常才展开 banner
	if (!isIssue) {
		return (
			<div className="flex flex-wrap items-center gap-x-3 gap-y-1 text-sm">
				<span
					className={cn(
						"size-2 shrink-0 rounded-full",
						health.status === "healthy"
							? "bg-emerald-500"
							: "bg-muted-foreground/40",
					)}
				/>
				<p className="font-medium">{t(presentation.labelKey)}</p>
				<p className="text-muted-foreground">{message}</p>
				<span className="text-xs text-muted-foreground">{checkedAt}</span>
				{historyButton ? (
					<span className="ml-auto">{historyButton}</span>
				) : null}
			</div>
		);
	}

	return (
		<div
			className={cn(
				"flex flex-col gap-3 rounded-xl px-4 py-3 md:flex-row md:items-start md:justify-between",
				presentation.className,
			)}
		>
			<div className="flex min-w-0 items-start gap-3">
				<span
					className={cn(
						"mt-0.5 flex size-8 shrink-0 items-center justify-center rounded-full",
						presentation.iconClass,
					)}
				>
					<Icon name={presentation.icon} className="size-4" />
				</span>
				<div className="min-w-0 space-y-1">
					<div className="flex flex-wrap items-center gap-2">
						<p className="text-sm font-semibold">{t(presentation.labelKey)}</p>
						<Badge
							variant="outline"
							className="border-current/25 bg-background/40"
						>
							{checkedAt}
						</Badge>
					</div>
					<p className="break-words text-sm">{message}</p>
					{issueComponents.length > 0 ? (
						<div className="mt-2 flex flex-wrap gap-2">
							{issueComponents.map((component) => (
								<Badge
									key={component.name}
									variant="outline"
									className="max-w-full border-current/25 bg-background/40"
									title={component.message}
								>
									<span className="truncate">
										{formatIssueComponent(component)}
									</span>
								</Badge>
							))}
						</div>
					) : null}
					{issueComponents.length === 0 && health.details ? (
						<p className="break-words text-xs opacity-80">{health.details}</p>
					) : null}
				</div>
			</div>
			{historyButton}
		</div>
	);
}
