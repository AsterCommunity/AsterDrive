import { useTranslation } from "react-i18next";
import {
	ADMIN_INTERACTIVE_TABLE_ROW_CLASS,
	ADMIN_TABLE_BADGE_CELL_CLASS,
	ADMIN_TABLE_MONO_TEXT_CLASS,
	ADMIN_TABLE_MUTED_TEXT_CLASS,
	ADMIN_TABLE_STACKED_CELL_CLASS,
	ADMIN_TABLE_TEXT_CELL_CLASS,
	AdminTable,
	AdminTableBody,
	AdminTableShell,
	AdminTableCell as TableCell,
	AdminTableHead as TableHead,
	AdminTableHeader as TableHeader,
	AdminTableRow as TableRow,
} from "@/components/common/AdminTable";
import { Badge } from "@/components/ui/badge";
import { Button } from "@/components/ui/button";
import { Icon } from "@/components/ui/icon";
import { ADMIN_ICON_BUTTON_CLASS } from "@/lib/constants";
import { formatDateAbsolute, formatDateAbsoluteWithOffset } from "@/lib/format";
import { cn } from "@/lib/utils";
import type {
	AdminExternalAuthProviderInfo,
	AdminExternalAuthProviderKindInfo,
} from "@/types/api";
import {
	callbackUrl,
	ExternalAuthProviderIcon,
	kindDisplayName,
	providerAllowedDomainSummary,
	providerPrimaryEndpoint,
	providerStatusTone,
	securityModeLabel,
} from "./shared";

interface ExternalAuthProvidersTableProps {
	deletingId: number | null;
	items: AdminExternalAuthProviderInfo[];
	onCopyCallbackUrl: (value: string) => void;
	onEdit: (provider: AdminExternalAuthProviderInfo) => void;
	onRequestDelete: (id: number) => void;
	onTestProvider: (provider: AdminExternalAuthProviderInfo) => void;
	providerKinds: AdminExternalAuthProviderKindInfo[];
	testingId: number | null;
}

export function ExternalAuthProvidersTable({
	deletingId,
	items,
	onCopyCallbackUrl,
	onEdit,
	onRequestDelete,
	onTestProvider,
	providerKinds,
	testingId,
}: ExternalAuthProvidersTableProps) {
	const { t } = useTranslation("admin");

	return (
		<AdminTableShell>
			<AdminTable>
				<TableHeader>
					<TableRow>
						<TableHead className="w-16">{t("id")}</TableHead>
						<TableHead className="min-w-[220px]">
							{t("external_auth_provider")}
						</TableHead>
						<TableHead className="min-w-[260px]">
							{t("external_auth_provider_primary_endpoint")}
						</TableHead>
						<TableHead className="w-[180px]">
							{t("external_auth_provider_allowed_domains")}
						</TableHead>
						<TableHead className="w-[220px]">{t("core:status")}</TableHead>
						<TableHead className="w-32">{t("core:actions")}</TableHead>
					</TableRow>
				</TableHeader>
				<AdminTableBody>
					{items.map((provider) => {
						const deleting = deletingId === provider.id;
						const testing = testingId === provider.id;
						const providerCallbackUrl = callbackUrl(
							provider.provider_kind,
							provider.key,
						);
						const primaryEndpoint = providerPrimaryEndpoint(provider);
						const allowedDomainSummary = providerAllowedDomainSummary(
							t,
							provider,
						);
						return (
							<TableRow
								key={provider.id}
								className={ADMIN_INTERACTIVE_TABLE_ROW_CLASS}
								onClick={() => {
									if (!deleting) onEdit(provider);
								}}
								onKeyDown={(event) => {
									if (event.key === "Enter" || event.key === " ") {
										event.preventDefault();
										if (!deleting) onEdit(provider);
									}
								}}
								tabIndex={0}
							>
								<TableCell>
									<div className={ADMIN_TABLE_TEXT_CELL_CLASS}>
										<span className={ADMIN_TABLE_MONO_TEXT_CLASS}>
											{provider.id}
										</span>
									</div>
								</TableCell>
								<TableCell>
									<div className={ADMIN_TABLE_TEXT_CELL_CLASS}>
										<div className="mr-3 flex size-10 shrink-0 items-center justify-center rounded-xl bg-white shadow-xs ring-1 ring-black/5">
											<ExternalAuthProviderIcon
												kind={provider.provider_kind}
												iconUrl={provider.icon_url}
												className="max-h-7 max-w-7"
											/>
										</div>
										<div className={ADMIN_TABLE_STACKED_CELL_CLASS}>
											<div className="flex min-w-0 items-center gap-2">
												<span className="truncate font-medium text-foreground">
													{provider.display_name}
												</span>
											</div>
											<div className="flex min-w-0 flex-wrap items-center gap-2">
												<Badge variant="outline">
													{kindDisplayName(
														t,
														provider.provider_kind,
														providerKinds,
													)}
												</Badge>
											</div>
										</div>
									</div>
								</TableCell>
								<TableCell>
									<div className={ADMIN_TABLE_STACKED_CELL_CLASS}>
										<span
											className="truncate font-mono text-xs text-foreground"
											title={primaryEndpoint?.value ?? "-"}
										>
											{primaryEndpoint?.value ?? "-"}
										</span>
										<span
											className={ADMIN_TABLE_MUTED_TEXT_CLASS}
											title={formatDateAbsoluteWithOffset(provider.updated_at)}
										>
											{primaryEndpoint
												? t(primaryEndpoint.labelKey)
												: t("external_auth_provider_primary_endpoint")}
											{" · "}
											{t("core:updated_at")}:{" "}
											{formatDateAbsolute(provider.updated_at)}
										</span>
									</div>
								</TableCell>
								<TableCell>
									<div className={ADMIN_TABLE_STACKED_CELL_CLASS}>
										<span
											className="truncate text-xs text-muted-foreground"
											title={allowedDomainSummary}
										>
											{allowedDomainSummary}
										</span>
										<span
											className="truncate font-mono text-xs text-muted-foreground"
											title={provider.scopes}
										>
											{provider.scopes}
										</span>
									</div>
								</TableCell>
								<TableCell>
									<div className={ADMIN_TABLE_BADGE_CELL_CLASS}>
										<Badge
											variant="outline"
											className={providerStatusTone(provider)}
										>
											{provider.enabled
												? t("external_auth_provider_enabled_badge")
												: t("external_auth_provider_disabled_badge")}
										</Badge>
										<Badge variant="outline">
											{securityModeLabel(t, provider)}
										</Badge>
										{provider.require_email_verified ? (
											<Badge variant="outline">
												{t("external_auth_provider_require_email_verified")}
											</Badge>
										) : null}
									</div>
								</TableCell>
								<TableCell
									onClick={(event) => event.stopPropagation()}
									onKeyDown={(event) => event.stopPropagation()}
								>
									<div className="flex justify-end gap-1">
										<Button
											variant="ghost"
											size="icon"
											className={ADMIN_ICON_BUTTON_CLASS}
											onClick={() => onCopyCallbackUrl(providerCallbackUrl)}
											disabled={!providerCallbackUrl || deleting}
											aria-label={t("external_auth_provider_copy_callback_url")}
											title={t("external_auth_provider_copy_callback_url")}
										>
											<Icon name="Copy" className="size-3.5" />
										</Button>
										<Button
											variant="ghost"
											size="icon"
											className={ADMIN_ICON_BUTTON_CLASS}
											onClick={() => onTestProvider(provider)}
											disabled={testing || deleting}
											aria-label={t("external_auth_provider_test")}
											title={t("external_auth_provider_test")}
										>
											<Icon
												name={testing ? "Spinner" : "WifiHigh"}
												className={cn("size-3.5", testing && "animate-spin")}
											/>
										</Button>
										<Button
											variant="ghost"
											size="icon"
											className={`${ADMIN_ICON_BUTTON_CLASS} text-destructive`}
											onClick={() => onRequestDelete(provider.id)}
											disabled={deleting || testing}
											aria-label={t("external_auth_provider_delete")}
											title={t("external_auth_provider_delete")}
										>
											<Icon
												name={deleting ? "Spinner" : "Trash"}
												className={cn("size-3.5", deleting && "animate-spin")}
											/>
										</Button>
									</div>
								</TableCell>
							</TableRow>
						);
					})}
				</AdminTableBody>
			</AdminTable>
		</AdminTableShell>
	);
}
