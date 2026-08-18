import { useState } from "react";
import { useTranslation } from "react-i18next";
import { AnimatedCollapsible } from "@/components/common/AnimatedCollapsible";
import { SessionPlatformIcon } from "@/components/settings/SessionPlatformIcon";
import { Badge } from "@/components/ui/badge";
import { Button } from "@/components/ui/button";
import { Icon } from "@/components/ui/icon";
import { formatDateAbsolute, formatDateAbsoluteWithOffset } from "@/lib/format";
import { formatUserAgentLabel } from "@/lib/userAgent";
import type { AuthSessionInfo } from "@/types/api";

interface SecuritySessionsSectionProps {
	hasOtherSessions: boolean;
	revokeBusyId: string | null;
	revokeOthersBusy: boolean;
	sessions: AuthSessionInfo[];
	sessionsLoading: boolean;
	onRefreshSessions: () => void;
	onRevokeOtherSessions: () => void;
	onRevokeSession: (session: AuthSessionInfo) => void;
}

export function SecuritySessionsSection({
	hasOtherSessions,
	onRefreshSessions,
	onRevokeOtherSessions,
	onRevokeSession,
	revokeBusyId,
	revokeOthersBusy,
	sessions,
	sessionsLoading,
}: SecuritySessionsSectionProps) {
	const { t } = useTranslation(["auth", "core", "settings"]);
	const [expandedIds, setExpandedIds] = useState<Set<string>>(() => new Set());
	const sessionDeviceLabels = {
		desktop: t("settings:settings_sessions_device_desktop"),
		mobile: t("settings:settings_sessions_device_mobile"),
		tablet: t("settings:settings_sessions_device_tablet"),
		unknown: t("settings:settings_sessions_unknown_device"),
	};
	const toggleExpanded = (id: string) => {
		setExpandedIds((previous) => {
			const next = new Set(previous);
			if (next.has(id)) {
				next.delete(id);
			} else {
				next.add(id);
			}
			return next;
		});
	};

	return (
		<div className="space-y-4">
			<div className="flex flex-col gap-3 md:flex-row md:items-start md:justify-between">
				<div className="space-y-1">
					<h3 className="text-sm font-semibold">
						{t("settings:settings_sessions_section")}
					</h3>
					<p className="text-sm text-muted-foreground">
						{t("settings:settings_sessions_section_desc")}
					</p>
				</div>
				<div className="flex flex-wrap gap-2">
					<Button
						type="button"
						variant="outline"
						disabled={sessionsLoading}
						onClick={onRefreshSessions}
					>
						{sessionsLoading ? (
							<Icon name="Spinner" className="mr-2 size-4 animate-spin" />
						) : (
							<Icon name="ArrowClockwise" className="mr-2 size-4" />
						)}
						{t("core:refresh")}
					</Button>
					<Button
						type="button"
						variant="outline"
						disabled={revokeOthersBusy || !hasOtherSessions}
						onClick={onRevokeOtherSessions}
					>
						{revokeOthersBusy ? (
							<Icon name="Spinner" className="mr-2 size-4 animate-spin" />
						) : (
							<Icon name="SignOut" className="mr-2 size-4" />
						)}
						{t("settings:settings_sessions_revoke_others")}
					</Button>
				</div>
			</div>

			{sessionsLoading ? (
				<div className="rounded-xl bg-muted/30 px-4 py-8 text-center text-sm text-muted-foreground">
					{t("core:loading")}
				</div>
			) : sessions.length === 0 ? (
				<div className="rounded-xl bg-muted/30 px-4 py-8 text-center">
					<p className="text-sm font-medium">
						{t("settings:settings_sessions_empty")}
					</p>
					<p className="mt-1 text-sm text-muted-foreground">
						{t("settings:settings_sessions_empty_desc")}
					</p>
				</div>
			) : (
				<div className="space-y-3">
					{sessions.map((session) => {
						const busy = revokeBusyId === session.id;
						const expanded = expandedIds.has(session.id);
						const sessionDeviceLabel = formatUserAgentLabel(
							session.user_agent,
							sessionDeviceLabels,
						);
						const sessionUserAgentTitle =
							session.user_agent?.trim() ||
							t("settings:settings_sessions_unknown_device");
						return (
							<div key={session.id} className="rounded-xl bg-muted/30 p-3">
								<div className="flex flex-col gap-3">
									<div className="flex flex-col gap-3 md:flex-row md:items-center md:justify-between">
										<div className="flex min-w-0 items-center gap-2">
											<div className="rounded-lg bg-background p-2">
												<SessionPlatformIcon userAgent={session.user_agent} />
											</div>
											<div className="min-w-0 flex-1 space-y-1">
												<p
													className="truncate text-sm font-semibold"
													title={sessionUserAgentTitle}
												>
													{sessionDeviceLabel}
												</p>
												<p className="text-xs text-muted-foreground">
													{t("settings:settings_sessions_last_seen")}:{" "}
													<span
														title={formatDateAbsoluteWithOffset(
															session.last_seen_at,
														)}
													>
														{formatDateAbsolute(session.last_seen_at)}
													</span>
												</p>
											</div>
											{session.is_current ? (
												<Badge variant="secondary">
													{t("settings:settings_sessions_current")}
												</Badge>
											) : null}
										</div>
										<div className="flex flex-wrap gap-2 md:justify-end">
											<Button
												type="button"
												size="sm"
												variant="ghost"
												aria-expanded={expanded}
												onClick={() => toggleExpanded(session.id)}
											>
												{expanded
													? t("settings:settings_security_hide_details")
													: t("settings:settings_security_show_details")}
											</Button>
											<Button
												type="button"
												size="sm"
												variant={session.is_current ? "destructive" : "outline"}
												disabled={busy}
												onClick={() => onRevokeSession(session)}
											>
												{busy ? (
													<Icon
														name="Spinner"
														className="mr-2 size-4 animate-spin"
													/>
												) : (
													<Icon name="SignOut" className="mr-2 size-4" />
												)}
												{session.is_current
													? t("settings:settings_sessions_revoke_current")
													: t("settings:settings_sessions_revoke")}
											</Button>
										</div>
									</div>
									<AnimatedCollapsible open={expanded}>
										<div className="grid gap-2 border-t pt-3 text-xs text-muted-foreground md:grid-cols-3">
											<p>
												{t("settings:settings_sessions_signed_in")}:{" "}
												<span
													title={formatDateAbsoluteWithOffset(
														session.created_at,
													)}
												>
													{formatDateAbsolute(session.created_at)}
												</span>
											</p>
											<p>
												{t("settings:settings_sessions_expires")}:{" "}
												<span
													title={formatDateAbsoluteWithOffset(
														session.expires_at,
													)}
												>
													{formatDateAbsolute(session.expires_at)}
												</span>
											</p>
											<p>
												{t("settings:settings_sessions_ip")}:{" "}
												{session.ip_address ||
													t("settings:settings_sessions_unknown_ip")}
											</p>
										</div>
									</AnimatedCollapsible>
								</div>
							</div>
						);
					})}
				</div>
			)}
		</div>
	);
}
