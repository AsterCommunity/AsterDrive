import type { ChangeEvent } from "react";
import { useEffect, useMemo, useReducer, useState } from "react";
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
import { logger } from "@/lib/logger";
import { cn } from "@/lib/utils";
import {
	isTeamWorkspace,
	PERSONAL_WORKSPACE,
	type Workspace,
	workspaceEquals,
	workspaceKey,
	workspaceRootPath,
} from "@/lib/workspace";
import { teamService } from "@/services/teamService";
import { useTeamStore } from "@/stores/teamStore";
import { useWorkspaceStore } from "@/stores/workspaceStore";
import type { TeamInfo } from "@/types/api";

type WorkspaceSwitcherVariant = "topbar" | "sidebar";
const TEAM_SEARCH_LIMIT = 50;
const WORKSPACE_SWITCHER_RESTORE_OPEN_MS = 5_000;
const workspaceSwitcherRestoreOpenUntil: Record<
	WorkspaceSwitcherVariant,
	number
> = {
	sidebar: 0,
	topbar: 0,
};

function shouldRestoreWorkspaceSwitcherOpen(variant: WorkspaceSwitcherVariant) {
	return Date.now() <= workspaceSwitcherRestoreOpenUntil[variant];
}

interface TeamSearchState {
	query: string;
	searchedTeams: TeamInfo[] | null;
}

type TeamSearchAction =
	| { type: "queryChanged"; value: string }
	| { type: "searchFailed"; query: string }
	| { type: "searchSucceeded"; query: string; teams: TeamInfo[] };

const initialTeamSearchState: TeamSearchState = {
	query: "",
	searchedTeams: null,
};

function teamSearchReducer(
	state: TeamSearchState,
	action: TeamSearchAction,
): TeamSearchState {
	switch (action.type) {
		case "queryChanged":
			return {
				query: action.value,
				searchedTeams: null,
			};
		case "searchFailed":
			if (state.query.trim() !== action.query) return state;
			return {
				...state,
				searchedTeams: [],
			};
		case "searchSucceeded":
			if (state.query.trim() !== action.query) return state;
			return {
				...state,
				searchedTeams: action.teams,
			};
	}
}

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
			closeOnClick={false}
			value={value}
			className="min-h-10 rounded-xl py-1.5 pr-8 pl-2"
		>
			<span className="flex min-w-0 flex-1 items-center gap-2.5">
				<span
					className={cn(
						"flex size-7 shrink-0 items-center justify-center rounded-full border border-border/50 bg-muted/25 text-muted-foreground",
						active && "border-primary/30 bg-primary/10 text-primary",
					)}
				>
					<Icon name={icon} className="size-3.5" />
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
	const [menuOpen, setMenuOpen] = useState(() =>
		shouldRestoreWorkspaceSwitcherOpen(variant),
	);
	const [teamSearchState, dispatchTeamSearch] = useReducer(
		teamSearchReducer,
		initialTeamSearchState,
	);
	const normalizedTeamQuery = teamSearchState.query.trim();
	const hasTeamQuery = normalizedTeamQuery.length > 0;

	useEffect(() => {
		if (!normalizedTeamQuery) {
			return;
		}

		let active = true;

		const timer = window.setTimeout(() => {
			teamService
				.list({ keyword: normalizedTeamQuery, limit: TEAM_SEARCH_LIMIT })
				.then((nextTeams) => {
					if (active) {
						dispatchTeamSearch({
							type: "searchSucceeded",
							query: normalizedTeamQuery,
							teams: nextTeams,
						});
					}
				})
				.catch((error) => {
					if (!active) {
						return;
					}
					logger.warn("Failed to search teams", error);
					dispatchTeamSearch({
						type: "searchFailed",
						query: normalizedTeamQuery,
					});
				});
		}, 250);

		return () => {
			active = false;
			window.clearTimeout(timer);
		};
	}, [normalizedTeamQuery]);

	const activeTeam = isTeamWorkspace(workspace)
		? (teams.find((team) => team.id === workspace.teamId) ?? null)
		: null;
	const visibleTeams = useMemo(
		() => (hasTeamQuery ? (teamSearchState.searchedTeams ?? []) : teams),
		[hasTeamQuery, teamSearchState.searchedTeams, teams],
	);
	const currentLabel = isTeamWorkspace(workspace)
		? (activeTeam?.name ??
			t("workspace_team_fallback", { id: workspace.teamId }))
		: t("my_drive");
	const currentWorkspaceKey = workspaceKey(workspace);
	const triggerLabel = t("workspace_switcher_label", { name: currentLabel });

	const handleMenuOpenChange = (open: boolean) => {
		if (!open) {
			workspaceSwitcherRestoreOpenUntil[variant] = 0;
		}
		setMenuOpen(open);
	};

	const handleSelectWorkspace = (nextWorkspace: Workspace) => {
		if (workspaceEquals(workspace, nextWorkspace)) {
			return;
		}
		workspaceSwitcherRestoreOpenUntil[variant] =
			Date.now() + WORKSPACE_SWITCHER_RESTORE_OPEN_MS;
		setMenuOpen(true);
		navigate(workspaceRootPath(nextWorkspace));
	};
	const handleManageTeams = () => {
		workspaceSwitcherRestoreOpenUntil[variant] = 0;
		setMenuOpen(false);
		navigate("/settings/teams");
	};
	const handleTeamQueryChange = (event: ChangeEvent<HTMLInputElement>) => {
		dispatchTeamSearch({
			type: "queryChanged",
			value: event.target.value,
		});
	};
	const searchPending = hasTeamQuery && teamSearchState.searchedTeams === null;
	const isLoadingTeamOptions = loadingTeams || searchPending;
	const showLoadingState =
		(loadingTeams && teams.length === 0 && !hasTeamQuery) || searchPending;
	const showEmptyTeamsState =
		!loadingTeams && teams.length === 0 && !hasTeamQuery;
	const showNoMatchesState =
		hasTeamQuery &&
		teamSearchState.searchedTeams !== null &&
		visibleTeams.length === 0;
	const isSidebarVariant = variant === "sidebar";

	return (
		<DropdownMenu open={menuOpen} onOpenChange={handleMenuOpenChange}>
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
						"flex size-6 shrink-0 items-center justify-center rounded-full border border-border/50 bg-muted/20 text-muted-foreground",
						isTeamWorkspace(workspace) && "text-primary",
					)}
				>
					<Icon
						name={isTeamWorkspace(workspace) ? "Cloud" : "House"}
						className="size-3.5"
					/>
				</span>
				<span className="min-w-0 flex-1">
					<span className="block truncate text-[13px] font-medium text-foreground sm:text-sm">
						{currentLabel}
					</span>
				</span>
				<Icon
					name={isLoadingTeamOptions ? "Spinner" : "CaretDown"}
					className={cn(
						"size-3 shrink-0 text-muted-foreground",
						isLoadingTeamOptions && "animate-spin",
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
							className="-translate-y-1/2 absolute top-1/2 left-3 size-3.5 text-muted-foreground"
						/>
						<Input
							value={teamSearchState.query}
							onChange={handleTeamQueryChange}
							onKeyDown={(event) => {
								if (event.key !== "Tab") {
									event.stopPropagation();
								}
							}}
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
							{visibleTeams.map((team) => (
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
								<DropdownMenuItem disabled className="min-h-9 rounded-xl p-2">
									<Icon name="Spinner" className="size-3.5 animate-spin" />
									<span className="text-[13px] text-muted-foreground">
										{t("loading")}
									</span>
								</DropdownMenuItem>
							</>
						) : null}
						{showEmptyTeamsState || showNoMatchesState ? (
							<div className="flex min-h-28 flex-col items-center justify-center gap-2.5 p-6 text-center">
								<span className="flex size-9 items-center justify-center rounded-xl border border-border/70 bg-muted/20 text-muted-foreground">
									<Icon
										name={showNoMatchesState ? "MagnifyingGlass" : "Cloud"}
										className="size-4"
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
						className="min-h-10 rounded-xl p-2"
					>
						<span className="flex size-7 shrink-0 items-center justify-center rounded-full border border-border/50 bg-muted/25 text-muted-foreground">
							<Icon name="Gear" className="size-3.5" />
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
