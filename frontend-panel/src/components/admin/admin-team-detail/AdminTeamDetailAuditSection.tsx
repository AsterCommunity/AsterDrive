import { useEffect, useRef, useState } from "react";
import { useTranslation } from "react-i18next";
import { EmptyState } from "@/components/common/EmptyState";
import { SkeletonTable } from "@/components/common/SkeletonTable";
import { UserIdentity } from "@/components/common/UserIdentity";
import { Badge } from "@/components/ui/badge";
import { Button } from "@/components/ui/button";
import { Icon } from "@/components/ui/icon";
import { handleApiError } from "@/hooks/useApiError";
import { formatAuditDetail, formatAuditSummary } from "@/lib/audit";
import { formatDateAbsolute } from "@/lib/format";
import { formatTeamAuditSummary } from "@/lib/team";
import { adminTeamService } from "@/services/adminService";
import type { TeamAuditEntryInfo, TeamMemberRole } from "@/types/api";

interface AuditSectionProps {
	teamId: number;
	auditCurrentPage: number;
	auditEntries: TeamAuditEntryInfo[];
	auditLoading: boolean;
	auditOffset: number;
	auditTotal: number;
	auditTotalPages: number;
	nextAuditPageDisabled: boolean;
	prevAuditPageDisabled: boolean;
	roleLabel: (role: TeamMemberRole) => string;
	setAuditOffset: (offset: number | ((offset: number) => number)) => void;
}

export function AdminTeamDetailAuditSection({
	teamId,
	auditCurrentPage,
	auditEntries,
	auditLoading,
	auditOffset: _auditOffset,
	auditTotal,
	auditTotalPages,
	nextAuditPageDisabled,
	prevAuditPageDisabled,
	roleLabel,
	setAuditOffset,
}: AuditSectionProps) {
	const { t } = useTranslation(["admin", "core", "settings"]);
	const [exporting, setExporting] = useState(false);
	const mountedRef = useRef(true);
	useEffect(
		() => () => {
			mountedRef.current = false;
		},
		[],
	);
	const handleExport = async () => {
		if (exporting) return;
		setExporting(true);
		try {
			await adminTeamService.exportAuditLogs(teamId);
		} catch (error) {
			handleApiError(error);
		} finally {
			if (mountedRef.current) setExporting(false);
		}
	};

	return (
		<section>
			<div className="mb-5 flex flex-wrap items-start justify-between gap-3">
				<div>
					<h4 className="text-base font-semibold text-foreground">
						{t("team_audit_title")}
					</h4>
					<p className="mt-1 text-sm text-muted-foreground">
						{t("team_audit_desc")}
					</p>
				</div>
				<Button
					type="button"
					variant="outline"
					size="sm"
					onClick={() => void handleExport()}
					disabled={exporting}
				>
					<Icon
						name={exporting ? "Spinner" : "Download"}
						className={`mr-1 size-4 ${exporting ? "animate-spin" : ""}`}
					/>
					{t("core:export_csv")}
				</Button>
			</div>
			{auditLoading && auditEntries.length === 0 ? (
				<SkeletonTable columns={4} rows={4} />
			) : auditTotal === 0 ? (
				<EmptyState
					icon={<Icon name="Scroll" className="size-10" />}
					title={t("team_audit_empty")}
					description={t("team_audit_empty_desc")}
				/>
			) : (
				<>
					<div className="divide-y">
						{auditEntries.map((entry) => {
							const summary =
								formatAuditDetail(t, entry) ??
								formatTeamAuditSummary(entry, roleLabel);

							return (
								<div key={entry.id} className="py-4 first:pt-0">
									<div className="flex flex-col gap-3 md:flex-row md:items-start md:justify-between">
										<div className="space-y-2">
											<div className="flex flex-wrap items-center gap-2">
												<Badge variant="outline">
													{formatAuditSummary(t, entry)}
												</Badge>
												<UserIdentity user={entry.actor} />
											</div>
											<p className="text-sm text-muted-foreground">
												{formatDateAbsolute(entry.created_at)}
											</p>
											{summary ? (
												<p className="text-sm text-muted-foreground">
													{summary}
												</p>
											) : null}
										</div>
									</div>
								</div>
							);
						})}
					</div>
					{auditTotal > 10 ? (
						<div className="mt-4 flex items-center justify-between gap-3 text-sm text-muted-foreground">
							<span>
								{t("entries_page", {
									total: auditTotal,
									current: auditCurrentPage,
									pages: auditTotalPages,
								})}
							</span>
							<div className="flex items-center gap-2">
								<Button
									type="button"
									variant="outline"
									size="sm"
									disabled={prevAuditPageDisabled || auditLoading}
									onClick={() =>
										setAuditOffset((currentOffset) =>
											Math.max(0, currentOffset - 10),
										)
									}
								>
									<Icon name="CaretLeft" className="size-4" />
								</Button>
								<Button
									type="button"
									variant="outline"
									size="sm"
									disabled={nextAuditPageDisabled || auditLoading}
									onClick={() =>
										setAuditOffset((currentOffset) => currentOffset + 10)
									}
								>
									<Icon name="CaretRight" className="size-4" />
								</Button>
							</div>
						</div>
					) : null}
				</>
			)}
		</section>
	);
}
