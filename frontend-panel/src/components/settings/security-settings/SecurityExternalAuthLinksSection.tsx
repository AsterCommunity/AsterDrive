import { useCallback, useEffect, useState } from "react";
import { useTranslation } from "react-i18next";
import { toast } from "sonner";
import { AnimatedCollapsible } from "@/components/common/AnimatedCollapsible";
import { ConfirmDialog } from "@/components/common/ConfirmDialog";
import { Badge } from "@/components/ui/badge";
import { Button } from "@/components/ui/button";
import { Icon } from "@/components/ui/icon";
import { handleApiError } from "@/hooks/useApiError";
import { useConfirmDialog } from "@/hooks/useConfirmDialog";
import {
	externalAuthKindIconPath,
	normalizeExternalAuthIconUrl,
} from "@/lib/externalAuthProviders";
import { formatDateAbsolute, formatDateAbsoluteWithOffset } from "@/lib/format";
import { authService } from "@/services/authService";
import type { ExternalAuthLinkInfo } from "@/types/api";

function shortSubject(value: string) {
	if (value.length <= 24) return value;
	return `${value.slice(0, 10)}...${value.slice(-8)}`;
}

function optionalLabel(value: string | null | undefined, fallback: string) {
	const trimmed = value?.trim();
	return trimmed ? trimmed : fallback;
}

function ExternalAuthLinkIcon({ link }: { link: ExternalAuthLinkInfo }) {
	const configuredIcon = normalizeExternalAuthIconUrl(link.provider_icon_url);
	const kindIcon = externalAuthKindIconPath(link.provider_kind);
	const effectiveIcon = configuredIcon || kindIcon;
	const [iconSrc, setIconSrc] = useState(effectiveIcon);

	useEffect(() => {
		setIconSrc(effectiveIcon);
	}, [effectiveIcon]);

	if (iconSrc) {
		return (
			<img
				src={iconSrc}
				alt=""
				aria-hidden="true"
				className="size-5 object-contain"
				onError={(event) => {
					event.currentTarget.onerror = null;
					if (configuredIcon && iconSrc === configuredIcon && kindIcon) {
						setIconSrc(kindIcon);
						return;
					}
					setIconSrc("");
				}}
			/>
		);
	}

	return <Icon name="Globe" className="size-4" />;
}

export function SecurityExternalAuthLinksSection() {
	const { t } = useTranslation(["core", "settings"]);
	const [links, setLinks] = useState<ExternalAuthLinkInfo[]>([]);
	const [loading, setLoading] = useState(false);
	const [busyIds, setBusyIds] = useState<Set<number>>(() => new Set());
	const [expandedIds, setExpandedIds] = useState<Set<number>>(() => new Set());

	const loadLinks = useCallback(async (options?: { force?: boolean }) => {
		try {
			setLoading(true);
			setLinks(await authService.listExternalAuthLinks(options));
		} catch (error) {
			handleApiError(error);
		} finally {
			setLoading(false);
		}
	}, []);

	useEffect(() => {
		void loadLinks();
	}, [loadLinks]);

	const handleDelete = async (id: number) => {
		try {
			setBusyIds((previous) => new Set(previous).add(id));
			await authService.deleteExternalAuthLink(id);
			setLinks((prev) => prev.filter((link) => link.id !== id));
			toast.success(t("settings:settings_external_auth_links_deleted"));
		} catch (error) {
			handleApiError(error);
		} finally {
			setBusyIds((previous) => {
				const next = new Set(previous);
				next.delete(id);
				return next;
			});
		}
	};

	const { requestConfirm, dialogProps } =
		useConfirmDialog<number>(handleDelete);

	const toggleExpanded = (id: number) => {
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
		<div className="space-y-4 rounded-xl border bg-background p-4">
			<div className="flex flex-col gap-3 md:flex-row md:items-start md:justify-between">
				<div className="space-y-1">
					<h3 className="text-sm font-semibold">
						{t("settings:settings_external_auth_links_section")}
					</h3>
					<p className="text-sm text-muted-foreground">
						{t("settings:settings_external_auth_links_section_desc")}
					</p>
				</div>
				<Button
					type="button"
					variant="outline"
					disabled={loading}
					onClick={() => void loadLinks({ force: true })}
				>
					{loading ? (
						<Icon name="Spinner" className="mr-2 size-4 animate-spin" />
					) : (
						<Icon name="ArrowClockwise" className="mr-2 size-4" />
					)}
					{t("core:refresh")}
				</Button>
			</div>

			{loading ? (
				<div className="rounded-xl border border-dashed bg-muted/20 px-4 py-8 text-center text-sm text-muted-foreground">
					{t("core:loading")}
				</div>
			) : links.length === 0 ? (
				<div className="rounded-xl border border-dashed bg-muted/20 px-4 py-8 text-center">
					<p className="text-sm font-medium">
						{t("settings:settings_external_auth_links_empty")}
					</p>
					<p className="mt-1 text-sm text-muted-foreground">
						{t("settings:settings_external_auth_links_empty_desc")}
					</p>
				</div>
			) : (
				<div className="space-y-3">
					{links.map((link) => {
						const busy = busyIds.has(link.id);
						const expanded = expandedIds.has(link.id);
						const profileLabel = optionalLabel(
							link.display_name_snapshot,
							link.email_snapshot ?? link.provider_key,
						);

						return (
							<div key={link.id} className="rounded-xl border bg-muted/20 p-3">
								<div className="flex flex-col gap-3">
									<div className="flex flex-col gap-3 md:flex-row md:items-center md:justify-between">
										<div className="flex min-w-0 items-center gap-2">
											<div className="flex size-9 shrink-0 items-center justify-center rounded-lg border bg-background text-primary">
												<ExternalAuthLinkIcon link={link} />
											</div>
											<div className="min-w-0 flex-1 space-y-1">
												<p className="truncate text-sm font-semibold">
													{link.provider_display_name}
												</p>
												<p className="truncate text-xs text-muted-foreground">
													{profileLabel}
												</p>
											</div>
											<Badge variant="secondary">
												{shortSubject(link.subject)}
											</Badge>
										</div>
										<div className="flex flex-wrap gap-2 md:justify-end">
											<Button
												type="button"
												size="sm"
												variant="ghost"
												aria-expanded={expanded}
												onClick={() => toggleExpanded(link.id)}
											>
												{expanded
													? t("settings:settings_security_hide_details")
													: t("settings:settings_security_show_details")}
											</Button>
											<Button
												type="button"
												size="sm"
												variant="destructive"
												disabled={busy}
												onClick={() => requestConfirm(link.id)}
											>
												{busy ? (
													<Icon
														name="Spinner"
														className="mr-2 size-4 animate-spin"
													/>
												) : (
													<Icon name="Trash" className="mr-2 size-4" />
												)}
												{t("settings:settings_external_auth_links_delete")}
											</Button>
										</div>
									</div>
									<AnimatedCollapsible open={expanded}>
										<div className="grid gap-2 border-t pt-3 text-xs text-muted-foreground md:grid-cols-2">
											<p className="min-w-0">
												{t("settings:settings_external_auth_links_issuer")}:{" "}
												<span className="break-all" title={link.issuer}>
													{link.issuer}
												</span>
											</p>
											<p className="min-w-0">
												{t("settings:settings_external_auth_links_subject")}:{" "}
												<span className="break-all" title={link.subject}>
													{link.subject}
												</span>
											</p>
											<p>
												{t("settings:settings_external_auth_links_created")}:{" "}
												<span
													title={formatDateAbsoluteWithOffset(link.created_at)}
												>
													{formatDateAbsolute(link.created_at)}
												</span>
											</p>
											<p>
												{t("settings:settings_external_auth_links_last_login")}:{" "}
												{link.last_login_at ? (
													<span
														title={formatDateAbsoluteWithOffset(
															link.last_login_at,
														)}
													>
														{formatDateAbsolute(link.last_login_at)}
													</span>
												) : (
													t("settings:settings_external_auth_links_never_used")
												)}
											</p>
										</div>
									</AnimatedCollapsible>
								</div>
							</div>
						);
					})}
				</div>
			)}

			<ConfirmDialog
				{...dialogProps}
				title={t("settings:settings_external_auth_links_delete_title")}
				description={t("settings:settings_external_auth_links_delete_desc")}
				confirmLabel={t("settings:settings_external_auth_links_delete")}
				variant="destructive"
			/>
		</div>
	);
}
