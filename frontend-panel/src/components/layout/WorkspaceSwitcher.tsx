import { useDeferredValue, useMemo, useState } from "react";
import { useTranslation } from "react-i18next";
import { useNavigate } from "react-router-dom";
import { Button } from "@/components/ui/button";
import {
	DropdownMenu,
	DropdownMenuContent,
	DropdownMenuGroup,
	DropdownMenuItem,
	DropdownMenuLabel,
	DropdownMenuRadioGroup,
	DropdownMenuRadioItem,
	DropdownMenuSeparator,
	DropdownMenuTrigger,
} from "@/components/ui/dropdown-menu";
import { Icon } from "@/components/ui/icon";
import { Input } from "@/components/ui/input";
import { cn } from "@/lib/utils";
import {
	isTeamWorkspace,
	PERSONAL_WORKSPACE,
	type Workspace,
	workspaceEquals,
	workspaceKey,
	workspaceRootPath,
} from "@/lib/workspace";
import { useTeamStore } from "@/stores/teamStore";
import { useWorkspaceStore } from "@/stores/workspaceStore";

type WorkspaceSwitcherVariant = "topbar" | "sidebar";

function WorkspaceMenuItem({
	active,
	icon,
	label,
	meta,
	value,
}: {
	active: boolean;
	icon: "House" | "Cloud";
	label: string;
	meta: string;
	value: string;
}) {
	return (
		<DropdownMenuRadioItem
			value={value}
			className="min-h-10 rounded-xl py-1.5 pr-8 pl-2"
		>
			<span className="flex min-w-0 flex-1 items-center gap-2.5">
				<span
					className={cn(
						"flex h-7 w-7 shrink-0 items-center justify-center rounded-full border border-border/50 bg-muted/25 text-muted-foreground",
						active && "border-primary/30 bg-primary/10 text-primary",
					)}
				>
					<Icon name={icon} className="h-3.5 w-3.5" />
				</span>
				<span className="min-w-0 flex-1">
					<span className="block truncate text-[13px] font-medium text-foreground">
						{label}
					</span>
					<span className="block truncate text-[11px] text-muted-foreground">
						{meta}
					</span>
				</span>
			</span>
		</DropdownMenuRadioItem>
	);
}

interface WorkspaceSwitcherProps {
	variant?: WorkspaceSwitcherVariant;
}

export function WorkspaceSwitcher({
	variant = "topbar",
}: WorkspaceSwitcherProps) {
	const { t } = useTranslation("core");
	const navigate = useNavigate();
	const workspace = useWorkspaceStore((state) => state.workspace);
	const teams = useTeamStore((state) => state.teams);
	const loadingTeams = useTeamStore((state) => state.loading);
	const [teamQuery, setTeamQuery] = useState("");
	const deferredTeamQuery = useDeferredValue(teamQuery);

	const activeTeam = isTeamWorkspace(workspace)
		? (teams.find((team) => team.id === workspace.teamId) ?? null)
		: null;
	const filteredTeams = useMemo(() => {
		const query = deferredTeamQuery.trim().toLocaleLowerCase();
		if (!query) {
			return teams;
		}
		return teams.filter((team) =>
			team.name.toLocaleLowerCase().includes(query),
		);
	}, [deferredTeamQuery, teams]);
	const currentLabel = isTeamWorkspace(workspace)
		? (activeTeam?.name ??
			t("workspace_team_fallback", { id: workspace.teamId }))
		: t("my_drive");
	const currentWorkspaceKey = workspaceKey(workspace);
	const triggerLabel = t("workspace_switcher_label", { name: currentLabel });

	const handleSelectWorkspace = (nextWorkspace: Workspace) => {
		if (workspaceEquals(workspace, nextWorkspace)) {
			return;
		}
		navigate(workspaceRootPath(nextWorkspace));
	};
	const handleManageTeams = () => {
		navigate("/settings/teams");
	};
	const hasTeamQuery = teamQuery.trim().length > 0;
	const showLoadingState = loadingTeams && teams.length === 0;
	const showEmptyTeamsState =
		!loadingTeams && teams.length === 0 && !hasTeamQuery;
	const showNoMatchesState =
		!loadingTeams && teams.length > 0 && filteredTeams.length === 0;
	const isSidebarVariant = variant === "sidebar";

	return (
		<DropdownMenu>
			<DropdownMenuTrigger
				render={
					<Button
						type="button"
						variant="outline"
						size="sm"
						aria-label={triggerLabel}
						className={cn(
							"items-center gap-1.5 border-border/45 bg-background/65 px-2 text-left shadow-none hover:bg-muted/45",
							isSidebarVariant
								? "h-10 w-full justify-start rounded-lg"
								: "h-9 max-w-[8.75rem] rounded-full min-[380px]:max-w-[10.5rem] sm:max-w-[11rem]",
						)}
					/>
				}
			>
				<span
					className={cn(
						"flex h-6 w-6 shrink-0 items-center justify-center rounded-full border border-border/50 bg-muted/20 text-muted-foreground",
						isTeamWorkspace(workspace) && "text-primary",
					)}
				>
					<Icon
						name={isTeamWorkspace(workspace) ? "Cloud" : "House"}
						className="h-3.5 w-3.5"
					/>
				</span>
				<span className="min-w-0 flex-1">
					<span className="block truncate text-[13px] font-medium text-foreground sm:text-sm">
						{currentLabel}
					</span>
				</span>
				<Icon
					name={loadingTeams ? "Spinner" : "CaretDown"}
					className={cn(
						"h-3 w-3 shrink-0 text-muted-foreground",
						loadingTeams && "animate-spin",
					)}
				/>
			</DropdownMenuTrigger>
			<DropdownMenuContent
				align={isSidebarVariant ? "center" : "start"}
				className="w-[min(20rem,calc(100vw-1.5rem))] min-w-[16rem] overflow-hidden p-0"
			>
				<div className="border-b border-border/60 p-2">
					<div className="relative">
						<Icon
							name="MagnifyingGlass"
							className="-translate-y-1/2 absolute top-1/2 left-3 h-3.5 w-3.5 text-muted-foreground"
						/>
						<Input
							value={teamQuery}
							onChange={(event) => setTeamQuery(event.target.value)}
							onKeyDown={(event) => event.stopPropagation()}
							placeholder={t("workspace_search_placeholder")}
							aria-label={t("workspace_search_placeholder")}
							className="h-9 rounded-xl border-transparent bg-muted/35 pr-3 pl-9 text-sm shadow-none focus-visible:bg-background"
						/>
					</div>
				</div>
				<div className="max-h-[min(14rem,var(--available-height))] overflow-y-auto p-2 sm:max-h-[min(16rem,var(--available-height))]">
					<DropdownMenuGroup>
						<DropdownMenuLabel className="px-2">
							{t("workspaces")}
						</DropdownMenuLabel>
						<DropdownMenuRadioGroup
							value={currentWorkspaceKey}
							onValueChange={(value) => {
								if (value === "personal") {
									handleSelectWorkspace(PERSONAL_WORKSPACE);
									return;
								}

								const teamId = Number(value.replace("team:", ""));
								if (Number.isSafeInteger(teamId) && teamId > 0) {
									handleSelectWorkspace({ kind: "team", teamId });
								}
							}}
						>
							<WorkspaceMenuItem
								active={!isTeamWorkspace(workspace)}
								icon="House"
								label={t("my_drive")}
								meta={t("workspace_personal_label")}
								value="personal"
							/>
							{filteredTeams.map((team) => (
								<WorkspaceMenuItem
									key={team.id}
									active={
										isTeamWorkspace(workspace) && workspace.teamId === team.id
									}
									icon="Cloud"
									label={team.name}
									meta={t("workspace_team_label")}
									value={workspaceKey({ kind: "team", teamId: team.id })}
								/>
							))}
						</DropdownMenuRadioGroup>
						{showLoadingState ? (
							<>
								<DropdownMenuSeparator />
								<DropdownMenuItem
									disabled
									className="min-h-9 rounded-xl px-2 py-2"
								>
									<Icon name="Spinner" className="h-3.5 w-3.5 animate-spin" />
									<span className="text-[13px] text-muted-foreground">
										{t("loading")}
									</span>
								</DropdownMenuItem>
							</>
						) : null}
						{showEmptyTeamsState || showNoMatchesState ? (
							<div className="flex min-h-28 flex-col items-center justify-center gap-2.5 px-6 py-6 text-center">
								<span className="flex h-9 w-9 items-center justify-center rounded-xl border border-border/70 bg-muted/20 text-muted-foreground">
									<Icon
										name={showNoMatchesState ? "MagnifyingGlass" : "Cloud"}
										className="h-4 w-4"
									/>
								</span>
								<p className="text-[13px] leading-5 text-muted-foreground">
									{showNoMatchesState
										? t("workspace_no_matching_teams")
										: t("workspace_empty_teams")}
								</p>
							</div>
						) : null}
					</DropdownMenuGroup>
				</div>
				<div className="border-t border-border/60 p-2">
					<DropdownMenuItem
						onClick={handleManageTeams}
						className="min-h-10 rounded-xl px-2 py-2"
					>
						<span className="flex h-7 w-7 shrink-0 items-center justify-center rounded-full border border-border/50 bg-muted/25 text-muted-foreground">
							<Icon name="Gear" className="h-3.5 w-3.5" />
						</span>
						<span className="min-w-0 flex-1">
							<span className="block truncate text-[13px] font-medium text-foreground">
								{t("workspace_manage_teams")}
							</span>
							<span className="block truncate text-[11px] text-muted-foreground">
								{t("workspace_manage_teams_desc")}
							</span>
						</span>
					</DropdownMenuItem>
				</div>
			</DropdownMenuContent>
		</DropdownMenu>
	);
}
