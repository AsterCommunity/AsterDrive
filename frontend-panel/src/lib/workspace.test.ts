import { describe, expect, it } from "vitest";
import {
	CATEGORY_ROUTE_SEGMENTS,
	PERSONAL_WORKSPACE,
	type Workspace,
	workspaceSwitchPath,
} from "./workspace";

const TEAM_3: Workspace = { kind: "team", teamId: 3 };
const TEAM_9: Workspace = { kind: "team", teamId: 9 };

describe("workspaceSwitchPath", () => {
	it.each([
		["/teams/3/search", "/teams/9/search"],
		["/teams/3/shares", "/teams/9/shares"],
		["/teams/3/tasks", "/teams/9/tasks"],
		["/teams/3/trash", "/teams/9/trash"],
		["/teams/3/search/", "/teams/9/search"],
		["/teams/3/trash/", "/teams/9/trash"],
	])(
		"preserves the workspace-agnostic team surface %s",
		(pathname, expected) => {
			expect(workspaceSwitchPath(TEAM_3, TEAM_9, { pathname })).toBe(expected);
		},
	);

	it.each(Object.values(CATEGORY_ROUTE_SEGMENTS))(
		"preserves the registered category surface %s",
		(category) => {
			expect(
				workspaceSwitchPath(TEAM_3, TEAM_9, {
					pathname: `/teams/3/category/${category}/`,
				}),
			).toBe(`/teams/9/category/${category}`);
		},
	);

	it("preserves query parameters and hashes on retained team surfaces", () => {
		expect(
			workspaceSwitchPath(TEAM_3, TEAM_9, {
				hash: "#results",
				pathname: "/teams/3/search",
				search: "?q=report&type=file",
			}),
		).toBe("/teams/9/search?q=report&type=file#results");
	});

	it.each([
		["/teams/3", "/teams/9"],
		["/teams/3/", "/teams/9"],
		["/teams/3/folder/42", "/teams/9"],
		["/teams/3/category/photo/extra", "/teams/9"],
		["/teams/3/category/unknown", "/teams/9"],
		["/teams/3/category/", "/teams/9"],
		["/teams/3/search/advanced", "/teams/9"],
		["/teams/3/search//", "/teams/9"],
		["/teams/3/unknown", "/teams/9"],
		["/teams/30/search", "/teams/9"],
		["/teams/4/search", "/teams/9"],
		["/settings/profile", "/teams/9"],
	])("falls back to the target root for %s", (pathname, expected) => {
		expect(
			workspaceSwitchPath(TEAM_3, TEAM_9, {
				hash: "#ignored",
				pathname,
				search: "?ignored=true",
			}),
		).toBe(expected);
	});

	it("returns the target root when switching from personal to team", () => {
		expect(
			workspaceSwitchPath(PERSONAL_WORKSPACE, TEAM_9, {
				pathname: "/search",
				search: "?q=report",
			}),
		).toBe("/teams/9");
	});

	it("returns the personal root when switching from team to personal", () => {
		expect(
			workspaceSwitchPath(TEAM_3, PERSONAL_WORKSPACE, {
				pathname: "/teams/3/trash",
			}),
		).toBe("/");
	});
});
