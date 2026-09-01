import type { ReactNode, RefObject } from "react";
import { useTranslation } from "react-i18next";
import { UserIdentity } from "@/components/common/UserIdentity";
import { Badge } from "@/components/ui/badge";
import { Button } from "@/components/ui/button";
import { Icon } from "@/components/ui/icon";
import { Progress } from "@/components/ui/progress";
import { Tabs, TabsContent, TabsList, TabsTrigger } from "@/components/ui/tabs";
import { formatBytes, formatDateAbsolute } from "@/lib/format";
import { cn } from "@/lib/utils";
import type { AdminTeamInfo } from "@/types/api";
import type { AdminTeamDetailTab } from "./types";

interface AdminTeamDetailShellProps {
	auditSection: ReactNode;
	contentRef: RefObject<HTMLDivElement | null>;
	currentPolicyGroupName: string | null;
	currentTab: AdminTeamDetailTab;
	dangerSection: ReactNode;
	membersSection: ReactNode;
	onContentScroll: () => void;
	onPageBack: () => void;
	onSidebarScroll: () => void;
	onTabChange: (value: string) => void;
	overviewSection: ReactNode;
	ownerCount: number;
	managerCount: number;
	panelAnimationClass: string;
	quota: number;
	selectedPolicyGroupName: string | null;
	sidebarRef: RefObject<HTMLElement | null>;
	team: AdminTeamInfo | null;
	usagePercentage: number;
	used: number;
}

export function AdminTeamDetailShell({
	auditSection,
	contentRef,
	currentPolicyGroupName,
	currentTab,
	dangerSection,
	membersSection,
	onContentScroll,
	onPageBack,
	onSidebarScroll,
	onTabChange,
	overviewSection,
	ownerCount,
	managerCount,
	panelAnimationClass,
	quota,
	selectedPolicyGroupName,
	sidebarRef,
	team,
	usagePercentage,
	used,
}: AdminTeamDetailShellProps) {
	const { t } = useTranslation(["admin", "core", "settings"]);
	return (
		<div className="flex min-h-0 flex-1 flex-col overflow-hidden">
			<div className="flex flex-wrap items-start justify-between gap-3 px-6 pb-4">
				<div className="space-y-1">
					<p className="text-xs uppercase tracking-wide text-muted-foreground">
						{t("teams")}
					</p>
					<h1 className="text-xl font-semibold tracking-tight">
						{team?.name ?? t("team_details_title")}
					</h1>
					<p className="text-sm text-muted-foreground">
						{t("team_details_title")}
					</p>
				</div>
				<Button
					type="button"
					variant="ghost"
					size="sm"
					className="-ml-2 -mt-2 text-muted-foreground"
					onClick={onPageBack}
				>
					<Icon name="ArrowLeft" className="mr-1 size-4" />
					{t("core:back")}
				</Button>
			</div>
			<div
				ref={contentRef}
				className="flex min-h-0 flex-1 flex-col overflow-y-auto lg:overflow-hidden"
				onScroll={onContentScroll}
			>
				<div className="flex min-h-full flex-col lg:h-full lg:min-h-0 lg:flex-1 lg:flex-row">
					<aside
						ref={sidebarRef}
						className="border-b lg:min-h-0 lg:w-80 lg:flex-none lg:overflow-y-auto lg:border-b-0"
						onScroll={onSidebarScroll}
					>
						<div className="space-y-5 p-6 max-lg:space-y-4 max-lg:p-4">
							<div className="flex flex-col gap-y-3 max-lg:flex-row max-lg:items-start max-lg:gap-3">
								<div className="flex size-16 items-center justify-center rounded-2xl bg-primary/10 text-primary max-lg:size-12 max-lg:rounded-xl">
									<Icon name="Cloud" className="size-7" />
								</div>
								<div className="space-y-3 max-lg:min-w-0 max-lg:flex-1">
									<div className="space-y-1">
										<h3 className="text-lg font-semibold text-foreground">
											{team?.name ?? t("core:loading")}
										</h3>
										<p className="text-sm text-muted-foreground max-lg:line-clamp-2">
											{team?.description || t("team_no_description")}
										</p>
									</div>
									<div className="flex flex-wrap gap-2">
										{team?.archived_at ? (
											<Badge variant="outline">{t("archived_badge")}</Badge>
										) : (
											<Badge variant="outline">{t("core:active")}</Badge>
										)}
										{team?.policy_group_id != null ? (
											<Badge variant="outline">
												{selectedPolicyGroupName ??
													currentPolicyGroupName ??
													`PG ${team.policy_group_id}`}
											</Badge>
										) : null}
									</div>
								</div>
							</div>

							<div className="grid gap-y-3 border-t pt-5 max-lg:grid-cols-2 max-lg:gap-3">
								<div className="space-y-1">
									<p className="text-xs uppercase tracking-wide text-muted-foreground">
										ID
									</p>
									<p className="font-mono text-sm text-foreground">
										{team?.id ?? "-"}
									</p>
								</div>
								<div className="space-y-1">
									<p className="text-xs uppercase tracking-wide text-muted-foreground">
										{t("created_by")}
									</p>
									<div className="min-w-0">
										{team ? <UserIdentity user={team.created_by} /> : "-"}
									</div>
								</div>
								<div className="space-y-1">
									<p className="text-xs uppercase tracking-wide text-muted-foreground">
										{t("core:created_at")}
									</p>
									<p className="text-sm text-foreground">
										{team ? formatDateAbsolute(team.created_at) : "-"}
									</p>
								</div>
								<div className="space-y-1">
									<p className="text-xs uppercase tracking-wide text-muted-foreground">
										{t("core:updated_at")}
									</p>
									<p className="text-sm text-foreground">
										{team ? formatDateAbsolute(team.updated_at) : "-"}
									</p>
								</div>
								{team?.archived_at ? (
									<div className="space-y-1">
										<p className="text-xs uppercase tracking-wide text-muted-foreground">
											{t("team_archived_at")}
										</p>
										<p className="text-sm text-foreground">
											{formatDateAbsolute(team.archived_at)}
										</p>
									</div>
								) : null}
							</div>
							<div className="space-y-3 border-t pt-5">
								<div>
									<p className="text-sm font-medium text-foreground">
										{t("storage")}
									</p>
									<p className="text-xs text-muted-foreground">
										{formatBytes(used)}
										{quota > 0
											? ` / ${formatBytes(quota)}`
											: ` / ${t("core:unlimited")}`}
									</p>
								</div>
								{quota > 0 ? (
									<Progress value={usagePercentage} className="h-2" />
								) : null}
								<div className="space-y-2 text-xs text-muted-foreground">
									<div className="flex items-center justify-between gap-3">
										<span>{t("member_count")}</span>
										<span>{team?.member_count ?? "-"}</span>
									</div>
									<div className="flex items-center justify-between gap-3">
										<span>{t("team_owner_count")}</span>
										<span>{ownerCount}</span>
									</div>
									<div className="flex items-center justify-between gap-3">
										<span>{t("team_manager_count")}</span>
										<span>{managerCount}</span>
									</div>
								</div>
							</div>
						</div>
					</aside>

					<div className="min-h-0 min-w-0 lg:flex lg:h-full lg:flex-1 lg:flex-col lg:overflow-hidden">
						<Tabs
							value={currentTab}
							onValueChange={onTabChange}
							className="flex flex-col lg:h-full lg:min-h-0 lg:flex-1 lg:overflow-hidden"
						>
							<div className="px-6 pt-6 max-lg:px-4 max-lg:pt-4 lg:shrink-0">
								<TabsList
									variant="line"
									className="h-auto w-full gap-5 border-b px-0 pb-2"
								>
									<TabsTrigger
										value="overview"
										className="h-10 min-w-0 flex-none rounded-none px-0"
									>
										{t("overview")}
									</TabsTrigger>
									<TabsTrigger
										value="members"
										className="h-10 min-w-0 flex-none rounded-none px-0"
									>
										{t("settings:settings_team_members")}
									</TabsTrigger>
									<TabsTrigger
										value="audit"
										className="h-10 min-w-0 flex-none rounded-none px-0"
									>
										{t("team_audit_title")}
									</TabsTrigger>
									<TabsTrigger
										value="danger"
										className="h-10 min-w-0 flex-none rounded-none px-0"
									>
										{t("team_danger_zone")}
									</TabsTrigger>
								</TabsList>
							</div>

							<div className="px-6 pt-4 pb-6 max-lg:px-4 max-lg:pt-3 max-lg:pb-4 lg:min-h-0 lg:flex-1 lg:overflow-y-auto">
								<TabsContent
									value="overview"
									className={cn(
										"outline-none",
										currentTab === "overview" && panelAnimationClass,
									)}
								>
									{overviewSection}
								</TabsContent>
								<TabsContent
									value="members"
									className={cn(
										"outline-none",
										currentTab === "members" && panelAnimationClass,
									)}
								>
									{membersSection}
								</TabsContent>
								<TabsContent
									value="audit"
									className={cn(
										"outline-none",
										currentTab === "audit" && panelAnimationClass,
									)}
								>
									{auditSection}
								</TabsContent>
								<TabsContent
									value="danger"
									className={cn(
										"outline-none",
										currentTab === "danger" && panelAnimationClass,
									)}
								>
									{dangerSection}
								</TabsContent>
							</div>
						</Tabs>
					</div>
				</div>
			</div>
		</div>
	);
}
