import { beforeEach, describe, expect, it, vi } from "vitest";
import { createWorkspaceActivityResource } from "./workspaceActivityService";

const mock = vi.hoisted(() => ({ get: vi.fn(), download: vi.fn() }));
vi.mock("@/services/http", () => ({
	api: { get: mock.get },
	downloadFile: mock.download,
}));
describe("workspace activity resources", () => {
	beforeEach(() => {
		vi.clearAllMocks();
	});
	it("keeps personal, team, and administrator targets separate", async () => {
		const signal = new AbortController().signal;
		const cases = [
			[
				{
					kind: "resource",
					workspace: { kind: "personal" },
					entityType: "file",
					entityId: 9,
				},
				"/files/9/activities",
			],
			[
				{
					kind: "resource",
					workspace: { kind: "team", teamId: 7 },
					entityType: "folder",
					entityId: 9,
				},
				"/teams/7/folders/9/activities",
			],
			[{ kind: "team", teamId: 7 }, "/teams/7/activities"],
			[{ kind: "team", teamId: 7, admin: true }, "/admin/teams/7/activities"],
		] as const;
		for (const [target, path] of cases) {
			await createWorkspaceActivityResource(target).list(30, signal);
			expect(mock.get).toHaveBeenLastCalledWith(path, {
				params: { limit: 50, before_id: 30 },
				signal,
			});
		}
	});
	it("exports the same authorized team resource and provides no public/resource export", async () => {
		const resource = createWorkspaceActivityResource({
			kind: "team",
			teamId: 7,
			admin: true,
		});
		await resource.export?.();
		expect(mock.download).toHaveBeenCalledWith(
			"/admin/teams/7/activities/export",
			undefined,
			"asterdrive-team-7-activities.csv",
		);
		expect(
			createWorkspaceActivityResource({
				kind: "resource",
				workspace: { kind: "personal" },
				entityType: "file",
				entityId: 9,
			}).export,
		).toBeUndefined();
	});
});
