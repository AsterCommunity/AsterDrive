import type { FormEvent } from "react";
import { useTranslation } from "react-i18next";
import {
	AdminSortableTableHead,
	AdminTable as Table,
	AdminTableBody as TableBody,
	AdminTableCell as TableCell,
	AdminTableHead as TableHead,
	AdminTableHeader as TableHeader,
	AdminTableRow as TableRow,
} from "@/components/common/AdminTable";
import { EmptyState } from "@/components/common/EmptyState";
import { SkeletonTable } from "@/components/common/SkeletonTable";
import { UserIdentity } from "@/components/common/UserIdentity";
import { Badge } from "@/components/ui/badge";
import { Button } from "@/components/ui/button";
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
import { formatDateShort } from "@/lib/format";
import type { SortOrder } from "@/lib/pagination";
import { getTeamRoleBadgeClass } from "@/lib/team";
import { cn } from "@/lib/utils";
import type { AdminTeamMemberSortBy } from "@/types/adminSort";
import type {
	AdminTeamInfo,
	TeamMemberInfo,
	TeamMemberRole,
	UserStatus,
} from "@/types/api";

interface MembersSectionProps {
	canMutateTeam: boolean;
	hasMemberFilters: boolean;
	managerCount: number;
	memberCurrentPage: number;
	memberIdentifier: string;
	memberLoading: boolean;
	memberMutating: boolean;
	memberOffset: number;
	memberQuery: string;
	memberRole: TeamMemberRole;
	memberRoleFilter: "__all__" | TeamMemberRole;
	memberSortBy: AdminTeamMemberSortBy;
	memberSortOrder: SortOrder;
	memberStatusFilter: "__all__" | UserStatus;
	memberTotal: number;
	memberTotalPages: number;
	members: TeamMemberInfo[];
	nextMemberPageDisabled: boolean;
	ownerCount: number;
	prevMemberPageDisabled: boolean;
	requestRemoveConfirm: (memberUserId: number) => void;
	roleFilterOptions: ReadonlyArray<{
		label: string;
		value: "__all__" | TeamMemberRole;
	}>;
	roleLabel: (role: TeamMemberRole) => string;
	roleOptions: TeamMemberRole[];
	setMemberIdentifier: (value: string) => void;
	setMemberOffset: (offset: number) => void;
	setMemberQuery: (value: string) => void;
	setMemberRole: (role: TeamMemberRole) => void;
	setMemberRoleFilter: (value: "__all__" | TeamMemberRole) => void;
	setMemberStatusFilter: (value: "__all__" | UserStatus) => void;
	statusFilterOptions: ReadonlyArray<{
		label: string;
		value: "__all__" | UserStatus;
	}>;
	team: AdminTeamInfo | null;
	onAddMember: (event: FormEvent<HTMLFormElement>) => void;
	onMemberSortChange: (
		sortBy: AdminTeamMemberSortBy,
		sortOrder: SortOrder,
	) => void;
	onUpdateMemberRole: (
		memberUserId: number,
		role: TeamMemberRole,
	) => Promise<void>;
}

export function AdminTeamDetailMembersSection({
	canMutateTeam,
	hasMemberFilters,
	managerCount,
	memberCurrentPage,
	memberIdentifier,
	memberLoading,
	memberMutating,
	memberOffset,
	memberQuery,
	memberRole,
	memberRoleFilter,
	memberSortBy,
	memberSortOrder,
	memberStatusFilter,
	memberTotal,
	memberTotalPages,
	members,
	nextMemberPageDisabled,
	ownerCount,
	prevMemberPageDisabled,
	requestRemoveConfirm,
	roleFilterOptions,
	roleLabel,
	roleOptions,
	setMemberIdentifier,
	setMemberOffset,
	setMemberQuery,
	setMemberRole,
	setMemberRoleFilter,
	setMemberStatusFilter,
	statusFilterOptions,
	team,
	onAddMember,
	onMemberSortChange,
	onUpdateMemberRole,
}: MembersSectionProps) {
	const { t } = useTranslation(["admin", "core", "settings"]);

	return (
		<section className="rounded-2xl border bg-background/60 p-6">
			<div className="mb-5 flex flex-col gap-3 lg:flex-row lg:items-start lg:justify-between">
				<div>
					<h4 className="text-base font-semibold text-foreground">
						{t("settings:settings_team_members")}
					</h4>
					<p className="mt-1 text-sm text-muted-foreground">
						{t("settings:settings_team_members_desc")}
					</p>
				</div>
				<div className="grid gap-2 sm:grid-cols-[minmax(220px,1fr)_160px_160px]">
					<Input
						value={memberQuery}
						onChange={(event) => {
							setMemberOffset(0);
							setMemberQuery(event.target.value);
						}}
						placeholder={t("team_member_search_placeholder")}
						className={ADMIN_CONTROL_HEIGHT_CLASS}
					/>
					<Select
						items={roleFilterOptions}
						value={memberRoleFilter}
						onValueChange={(value) => {
							setMemberOffset(0);
							setMemberRoleFilter(
								(value as "__all__" | TeamMemberRole) ?? "__all__",
							);
						}}
					>
						<SelectTrigger>
							<SelectValue />
						</SelectTrigger>
						<SelectContent>
							{roleFilterOptions.map((option) => (
								<SelectItem key={option.value} value={option.value}>
									{option.label}
								</SelectItem>
							))}
						</SelectContent>
					</Select>
					<Select
						items={statusFilterOptions}
						value={memberStatusFilter}
						onValueChange={(value) => {
							setMemberOffset(0);
							setMemberStatusFilter(
								(value as "__all__" | UserStatus) ?? "__all__",
							);
						}}
					>
						<SelectTrigger>
							<SelectValue />
						</SelectTrigger>
						<SelectContent>
							{statusFilterOptions.map((option) => (
								<SelectItem key={option.value} value={option.value}>
									{option.label}
								</SelectItem>
							))}
						</SelectContent>
					</Select>
				</div>
			</div>

			<div className="mb-4 flex flex-wrap items-center justify-between gap-3 rounded-xl border bg-muted/20 px-4 py-3 text-sm">
				<div className="flex flex-wrap gap-4 text-muted-foreground">
					<span>
						{t("member_filtered_count", {
							filtered: memberTotal,
							total: team?.member_count ?? memberTotal,
						})}
					</span>
					<span>
						{t("team_owner_count")}: {ownerCount}
					</span>
					<span>
						{t("team_manager_count")}: {managerCount}
					</span>
				</div>
				{hasMemberFilters ? (
					<Button
						type="button"
						variant="ghost"
						size="sm"
						onClick={() => {
							setMemberOffset(0);
							setMemberQuery("");
							setMemberRoleFilter("__all__");
							setMemberStatusFilter("__all__");
						}}
					>
						{t("clear_filters")}
					</Button>
				) : null}
			</div>

			{canMutateTeam ? (
				<form
					className="mb-4 grid gap-3 rounded-xl border bg-muted/20 p-4 md:grid-cols-[minmax(0,1fr)_180px_auto]"
					onSubmit={onAddMember}
				>
					<div className="space-y-2">
						<Label htmlFor="admin-team-member-identifier">
							{t("settings:settings_team_member_identifier")}
						</Label>
						<Input
							id="admin-team-member-identifier"
							value={memberIdentifier}
							disabled={memberMutating}
							placeholder={t("settings:settings_team_member_placeholder")}
							onChange={(event) => setMemberIdentifier(event.target.value)}
						/>
						<p className="text-xs text-muted-foreground">
							{t("settings:settings_team_member_identifier_desc")}
						</p>
					</div>
					<div className="space-y-2">
						<Label>{t("settings:settings_team_role_label")}</Label>
						<Select
							items={roleOptions.map((role) => ({
								label: roleLabel(role),
								value: role,
							}))}
							value={memberRole}
							onValueChange={(value) => setMemberRole(value as TeamMemberRole)}
						>
							<SelectTrigger>
								<SelectValue />
							</SelectTrigger>
							<SelectContent>
								{roleOptions.map((role) => (
									<SelectItem key={role} value={role}>
										{roleLabel(role)}
									</SelectItem>
								))}
							</SelectContent>
						</Select>
					</div>
					<div className="flex items-end">
						<Button
							type="submit"
							className="w-full"
							disabled={memberMutating || !memberIdentifier.trim()}
						>
							{t("settings:settings_team_add_member")}
						</Button>
					</div>
				</form>
			) : (
				<div className="mb-4 rounded-xl border border-dashed bg-muted/10 px-4 py-3 text-sm text-muted-foreground">
					{t("team_members_readonly_archived")}
				</div>
			)}

			{memberLoading && members.length === 0 ? (
				<SkeletonTable columns={6} rows={5} />
			) : memberTotal === 0 ? (
				<EmptyState
					icon={<Icon name="ListBullets" className="h-10 w-10" />}
					title={
						hasMemberFilters
							? t("team_member_filtered_empty")
							: t("settings:settings_team_no_members")
					}
					description={
						hasMemberFilters
							? t("team_member_filtered_empty_desc")
							: t("settings:settings_team_no_members_desc")
					}
				/>
			) : (
				<>
					<div className="overflow-x-auto rounded-lg border border-border/70">
						<Table>
							<TableHeader>
								<TableRow>
									<AdminSortableTableHead
										sortKey="username"
										sortBy={memberSortBy}
										sortOrder={memberSortOrder}
										onSortChange={onMemberSortChange}
									>
										{t("settings:settings_team_member")}
									</AdminSortableTableHead>
									<AdminSortableTableHead
										sortKey="email"
										sortBy={memberSortBy}
										sortOrder={memberSortOrder}
										onSortChange={onMemberSortChange}
									>
										{t("settings:settings_team_email")}
									</AdminSortableTableHead>
									<AdminSortableTableHead
										sortKey="status"
										sortBy={memberSortBy}
										sortOrder={memberSortOrder}
										onSortChange={onMemberSortChange}
									>
										{t("settings:settings_team_status")}
									</AdminSortableTableHead>
									<AdminSortableTableHead
										sortKey="role"
										sortBy={memberSortBy}
										sortOrder={memberSortOrder}
										onSortChange={onMemberSortChange}
									>
										{t("settings:settings_team_role_label")}
									</AdminSortableTableHead>
									<AdminSortableTableHead
										sortKey="created_at"
										sortBy={memberSortBy}
										sortOrder={memberSortOrder}
										onSortChange={onMemberSortChange}
									>
										{t("core:created_at")}
									</AdminSortableTableHead>
									<TableHead>{t("core:actions")}</TableHead>
								</TableRow>
							</TableHeader>
							<TableBody>
								{members.map((member) => {
									const canEditRole = canMutateTeam && !memberMutating;
									const canRemove = canMutateTeam && !memberMutating;

									return (
										<TableRow key={member.id}>
											<TableCell>
												<div className="space-y-2">
													<div className="flex items-center gap-2">
														<UserIdentity user={member.user} />
														<Badge
															className={cn(
																"border",
																getTeamRoleBadgeClass(member.role),
															)}
														>
															{roleLabel(member.role)}
														</Badge>
													</div>
												</div>
											</TableCell>
											<TableCell>{member.email}</TableCell>
											<TableCell>
												<Badge
													variant="outline"
													className={
														member.status === "active"
															? "border-green-500/60 bg-green-500/10 text-green-700 dark:text-green-300"
															: "border-amber-500/60 bg-amber-500/10 text-amber-700 dark:text-amber-300"
													}
												>
													{member.status === "active"
														? t("core:active")
														: t("core:disabled_status")}
												</Badge>
											</TableCell>
											<TableCell>
												{canEditRole ? (
													<Select
														items={roleOptions.map((role) => ({
															label: roleLabel(role),
															value: role,
														}))}
														value={member.role}
														onValueChange={(value) => {
															if (value && value !== member.role) {
																void onUpdateMemberRole(
																	member.user_id,
																	value as TeamMemberRole,
																);
															}
														}}
													>
														<SelectTrigger width="compact">
															<SelectValue />
														</SelectTrigger>
														<SelectContent>
															{roleOptions.map((role) => (
																<SelectItem key={role} value={role}>
																	{roleLabel(role)}
																</SelectItem>
															))}
														</SelectContent>
													</Select>
												) : (
													<span className="text-sm text-muted-foreground">
														{roleLabel(member.role)}
													</span>
												)}
											</TableCell>
											<TableCell className="text-sm text-muted-foreground">
												{formatDateShort(member.created_at)}
											</TableCell>
											<TableCell>
												{canRemove ? (
													<Button
														type="button"
														variant="ghost"
														size="sm"
														className="text-destructive"
														disabled={memberMutating}
														onClick={() => requestRemoveConfirm(member.user_id)}
													>
														{t("settings:settings_team_remove_member")}
													</Button>
												) : (
													<span className="text-xs text-muted-foreground">
														-
													</span>
												)}
											</TableCell>
										</TableRow>
									);
								})}
							</TableBody>
						</Table>
					</div>
					{memberTotal > 10 ? (
						<div className="mt-4 flex items-center justify-between gap-3 text-sm text-muted-foreground">
							<span>
								{t("entries_page", {
									total: memberTotal,
									current: memberCurrentPage,
									pages: memberTotalPages,
								})}
							</span>
							<div className="flex items-center gap-2">
								<Button
									type="button"
									variant="outline"
									size="sm"
									disabled={prevMemberPageDisabled || memberLoading}
									onClick={() =>
										setMemberOffset(Math.max(0, memberOffset - 10))
									}
								>
									<Icon name="CaretLeft" className="h-4 w-4" />
								</Button>
								<Button
									type="button"
									variant="outline"
									size="sm"
									disabled={nextMemberPageDisabled || memberLoading}
									onClick={() => setMemberOffset(memberOffset + 10)}
								>
									<Icon name="CaretRight" className="h-4 w-4" />
								</Button>
							</div>
						</div>
					) : null}
				</>
			)}
		</section>
	);
}
