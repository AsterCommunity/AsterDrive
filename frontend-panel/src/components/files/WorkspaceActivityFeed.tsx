import { useEffect, useMemo, useState } from "react";
import { useTranslation } from "react-i18next";
import { Button } from "@/components/ui/button";
import { handleApiError } from "@/hooks/useApiError";
import { useWorkspaceActivities } from "@/hooks/useWorkspaceActivities";
import { ensureI18nNamespaces } from "@/i18n";
import { formatAuditAction } from "@/lib/audit";
import { formatDateAbsolute } from "@/lib/format";
import { logger } from "@/lib/logger";
import {
	type ActivityTarget,
	createWorkspaceActivityResource,
} from "@/services/workspaceActivityService";

export function WorkspaceActivityFeed({
	target,
	enabled = true,
}: {
	target: ActivityTarget;
	enabled?: boolean;
}) {
	const { t, i18n } = useTranslation(["files", "admin"]);
	useEffect(() => {
		if (!enabled) return;
		void ensureI18nNamespaces(
			[{ namespace: "admin", parts: ["audit"] }],
			i18n.language,
		).catch((error) =>
			logger.warn("failed to load file activity translations", error),
		);
	}, [enabled, i18n.language]);
	const resource = useMemo(
		() => createWorkspaceActivityResource(target),
		[target],
	);
	const data = useWorkspaceActivities(resource, enabled);
	const [exporting, setExporting] = useState(false);
	const exportCsv = async () => {
		if (!resource.export || exporting) return;
		setExporting(true);
		try {
			await resource.export();
		} catch (error) {
			handleApiError(error);
		} finally {
			setExporting(false);
		}
	};
	return (
		<section
			className="space-y-3"
			aria-label={t("activity_title")}
			aria-busy={data.loading}
		>
			<div className="flex items-center justify-between gap-3">
				<h3 className="text-sm font-semibold">{t("activity_title")}</h3>
				{resource.export ? (
					<Button
						variant="ghost"
						size="sm"
						disabled={exporting}
						onClick={() => void exportCsv()}
					>
						{t("activity_export")}
					</Button>
				) : null}
			</div>
			{data.failed ? (
				<div role="alert" className="space-y-2 text-sm text-muted-foreground">
					<p>{t("activity_load_failed")}</p>
					<Button variant="outline" size="sm" onClick={() => void data.retry()}>
						{t("activity_retry")}
					</Button>
				</div>
			) : null}
			{data.loading ? (
				<p role="status" className="text-sm text-muted-foreground">
					{t("activity_loading")}
				</p>
			) : null}
			{!data.loading && !data.failed && data.items.length === 0 ? (
				<p className="text-sm text-muted-foreground">{t("activity_empty")}</p>
			) : null}
			<ol className="divide-y">
				{data.items.map((item) => {
					const details =
						item.details && typeof item.details === "object"
							? (item.details as Record<string, unknown>)
							: null;
					const previous = details?.source_path ?? details?.previous_name;
					const next = details?.target_path ?? details?.next_name;
					return (
						<li
							key={item.event_id}
							className="space-y-1 py-3 first:pt-0 last:pb-0"
						>
							<p className="break-words text-sm">
								<span className="font-medium">
									{item.actor.name || t("info_unknown_actor")}
								</span>
								{" · "}
								{formatAuditAction(t, item.action)}
								{" · "}
								{item.entity_name}
							</p>
							{typeof previous === "string" &&
							typeof next === "string" &&
							previous !== next ? (
								<p className="break-words text-xs text-muted-foreground">
									{previous} → {next}
								</p>
							) : null}
							<p className="text-xs text-muted-foreground">
								{formatDateAbsolute(item.created_at)}
							</p>
						</li>
					);
				})}
			</ol>
			{data.items.length > 0 || data.page > 1 ? (
				<nav
					className="flex items-center justify-between gap-3"
					aria-label={t("activity_pagination")}
				>
					<Button
						variant="outline"
						size="sm"
						disabled={data.loading || data.page <= 1}
						onClick={() => void data.previousPage()}
					>
						{t("activity_previous")}
					</Button>
					<span className="text-xs text-muted-foreground" aria-live="polite">
						{t("activity_page", { page: data.page })}
					</span>
					<Button
						variant="outline"
						size="sm"
						disabled={data.loading || data.next === null}
						onClick={() => void data.nextPage()}
					>
						{t("activity_next")}
					</Button>
				</nav>
			) : null}
		</section>
	);
}
