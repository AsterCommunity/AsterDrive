import type { FormEvent, SetStateAction } from "react";
import { useTranslation } from "react-i18next";
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
import {
	Table,
	TableBody,
	TableCell,
	TableHead,
	TableHeader,
	TableRow,
} from "@/components/ui/table";
import { ADMIN_CONTROL_HEIGHT_CLASS } from "@/lib/constants";
import { formatDateShort } from "@/lib/format";
import { getTeamRoleBadgeClass, isTeamOwner } from "@/lib/team";
import { cn } from "@/lib/utils";
import type {
	TeamInfo,
	TeamMemberInfo,
	TeamMemberRole,
	UserStatus,
} from "@/types/api";

interface MembersSectionProps {
	canAssignOwner: boolean;
	canManageTeam: boolean;
	currentUserId: number | null;
	hasMemberFilters: boolean;
	managerCount: number;
	memberCurrentPage: number;
	memberIdentifier: string;
	memberLoading: boolean;
	memberOffset: number;
	memberPageSize: number;
	memberQuery: string;
	memberRole: TeamMemberRole;
	memberRoleFilter: "__all__" | TeamMemberRole;
	memberStatusFilter: "__all__" | UserStatus;
	memberTotal: number;
	memberTotalPages: number;
	members: TeamMemberInfo[];
	mutating: boolean;
	nextMemberPageDisabled: boolean;
	onAddMember: (event: FormEvent<HTMLFormElement>) => void;
	onUpdateMemberRole: (
		memberUserId: number,
		role: TeamMemberRole,
	) => void | Promise<void>;
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
	setMemberOffset: (offset: SetStateAction<number>) => void;
	setMemberQuery: (value: string) => void;
	setMemberRole: (value: TeamMemberRole) => void;
	setMemberRoleFilter: (value: "__all__" | TeamMemberRole) => void;
	setMemberStatusFilter: (value: "__all__" | UserStatus) => void;
	statusFilterOptions: ReadonlyArray<{
		label: string;
		value: "__all__" | UserStatus;
	}>;
	team: TeamInfo | null;
	viewerRole: TeamMemberRole | null;
}

export function TeamManageMembersSection({
	canAssignOwner,
	canManageTeam,
	currentUserId,
	hasMemberFilters,
	managerCount,
	memberCurrentPage,
	memberIdentifier,
	memberLoading,
	memberOffset: _memberOffset,
	memberPageSize,
	memberQuery,
	memberRole,
	memberRoleFilter,
	memberStatusFilter,
	memberTotal,
	memberTotalPages,
	members,
	mutating,
	nextMemberPageDisabled,
	onAddMember,
	onUpdateMemberRole,
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
	viewerRole,
}: MembersSectionProps) {
	const { t } = useTranslation(["core", "settings"]);

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
						placeholder={t("settings:settings_team_member_search_placeholder")}
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
						{t("settings:settings_team_member_filtered_count", {
							filtered: memberTotal,
							total: team?.member_count ?? memberTotal,
						})}
					</span>
					<span>
						{t("settings:settings_team_owner_count")}: {ownerCount}
					</span>
					<span>
						{t("settings:settings_team_manager_count")}: {managerCount}
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
						{t("settings:settings_team_clear_filters")}
					</Button>
				) : null}
			</div>

			{canManageTeam ? (
				<form
					className="mb-4 grid gap-3 rounded-xl border bg-muted/20 p-4 md:grid-cols-[minmax(0,1fr)_180px_auto]"
					onSubmit={onAddMember}
				>
					<div className="space-y-2">
						<Label htmlFor="team-member-identifier">
							{t("settings:settings_team_member_identifier")}
						</Label>
						<Input
							id="team-member-identifier"
							value={memberIdentifier}
							disabled={mutating}
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
							disabled={mutating || !memberIdentifier.trim()}
						>
							{t("settings:settings_team_add_member")}
						</Button>
					</div>
				</form>
			) : null}

			{memberLoading && members.length === 0 ? (
				<SkeletonTable columns={6} rows={5} />
			) : memberTotal === 0 ? (
				<EmptyState
					icon={<Icon name="ListBullets" className="size-10" />}
					title={
						hasMemberFilters
							? t("settings:settings_team_member_filtered_empty")
							: t("settings:settings_team_no_members")
					}
					description={
						hasMemberFilters
							? t("settings:settings_team_member_filtered_empty_desc")
							: t("settings:settings_team_no_members_desc")
					}
				/>
			) : (
				<>
					<div className="overflow-x-auto rounded-xl border">
						<Table>
							<TableHeader>
								<TableRow>
									<TableHead>{t("settings:settings_team_member")}</TableHead>
									<TableHead>{t("settings:settings_team_email")}</TableHead>
									<TableHead>{t("settings:settings_team_status")}</TableHead>
									<TableHead>
										{t("settings:settings_team_role_label")}
									</TableHead>
									<TableHead>{t("core:created_at")}</TableHead>
									<TableHead>{t("core:actions")}</TableHead>
								</TableRow>
							</TableHeader>
							<TableBody>
								{members.map((member) => {
									const isSelf = member.user_id === currentUserId;
									const canRemoveSelf = isSelf && !isTeamOwner(viewerRole);
									const canManageOwner =
										canAssignOwner || member.role !== "owner";
									const canEditRole =
										canManageTeam && canManageOwner && !mutating;
									const canRemove =
										(canManageTeam && canManageOwner) || canRemoveSelf;

									return (
										<TableRow key={member.id}>
											<TableCell>
												<div className="space-y-2">
													<div className="flex items-center gap-2">
														<UserIdentity user={member.user} />
														{isSelf ? (
															<Badge variant="outline">
																{t("settings:settings_team_you")}
															</Badge>
														) : null}
														{!canEditRole ? (
															<Badge
																className={cn(
																	"border",
																	getTeamRoleBadgeClass(member.role),
																)}
															>
																{roleLabel(member.role)}
															</Badge>
														) : null}
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
														disabled={mutating}
														onClick={() => requestRemoveConfirm(member.user_id)}
													>
														{isSelf
															? t("settings:settings_team_leave")
															: t("settings:settings_team_remove_member")}
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
					{memberTotal > memberPageSize ? (
						<div className="mt-4 flex items-center justify-between gap-3 text-sm text-muted-foreground">
							<span>
								{t("settings:settings_team_entries_page", {
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
										setMemberOffset((current) =>
											Math.max(0, current - memberPageSize),
										)
									}
								>
									<Icon name="CaretLeft" className="size-4" />
								</Button>
								<Button
									type="button"
									variant="outline"
									size="sm"
									disabled={nextMemberPageDisabled || memberLoading}
									onClick={() =>
										setMemberOffset((current) => current + memberPageSize)
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
