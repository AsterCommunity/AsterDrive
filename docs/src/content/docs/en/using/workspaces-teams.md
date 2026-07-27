---
description: "AsterDrive workspaces and teams: the boundary between personal and team spaces, switching workspaces, team roles, team settings entries, and common triage order from the user perspective."
title: "Workspaces and Teams"
---

:::tip[What this page covers]
Workspaces and teams from the regular user's perspective: space boundaries, roles, and how to find team content. Admin operations such as creating teams, archiving, and policy group binding are in [Users and Teams](/en/admin/users-teams/).
:::

## Personal Space vs Team Space

The most important boundary in AsterDrive is the **workspace**.

| Item | Personal space | Team space |
| --- | --- | --- |
| File ownership | Current user | Current team |
| Shares | Only shares created in the personal space | Only shares created in the team space |
| Recycle bin | The personal space's own bin | Each team's own bin |
| Task center | Personal tasks | Team tasks |
| Search | Searches the current personal space | Searches the current team space |
| WebDAV | Personal accounts open only personal files | Team accounts open only that team's files |
| Storage route | Policy group bound to the user | Policy group bound to the team |

After you switch workspaces on the left, files, shares, tasks, recycle bin, and search results all switch with it. This is also the first step when troubleshooting "why can't I see a file / share / task": confirm which workspace you are currently in.

## Understanding Team Roles

Teams are usually understood in three roles:

| Role | Who it fits | What they can do |
| --- | --- | --- |
| Owner | Team lead | Manage the team, members, and team files |
| Admin | Day-to-day maintainer | Manage members and daily team content |
| Member | Regular collaborator | Use files, shares, and tasks in the team space |

:::tip[Check two layers for permission issues]
Whether a user can operate team content depends first on team membership, then on their role in the team. Do not just check whether they are a system administrator.
:::

## What `Settings -> Teams` Offers

`Settings -> Teams` lists the teams you have joined. You can:

- View team name, description, member count, and space usage
- Open a team workspace directly
- View archived teams

If you are an owner or admin of a team, you can also enter the team management page to handle members and team info, and view team audit; with the corresponding permission, you can restore archived teams.

## Day-to-Day Boundaries in Team Spaces

- A team space is not a separate site; it is served by the same AsterDrive service
- Team spaces have their own WebDAV accounts; the WebDAV address is global, and the account credentials decide whether a client enters the personal space or a team space
- Team quota and personal user quota are separate concepts
- Shares, recycle bin, tasks, and tag libraries in a team are all separate from the personal space
- When you cannot find "the share from earlier" or "the task from earlier" in a team, first confirm the current workspace is correct

## Common Triage Order

1. Is the user a team member?
2. Is the user's role in the team sufficient?
3. Is the current workspace the target team?
4. Is the team archived?
5. Do the relevant shares, tasks, or recycle bin items belong to the same team space?

The full admin triage path (policy groups, audit, tasks) is in [Users and Teams](/en/admin/users-teams/).
