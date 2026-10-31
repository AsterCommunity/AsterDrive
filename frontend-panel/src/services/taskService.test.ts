import { beforeEach, describe, expect, it, vi } from "vitest";
import { PERSONAL_WORKSPACE } from "@/lib/workspace";
import { createTaskService, taskService } from "@/services/taskService";
import { useWorkspaceStore } from "@/stores/workspaceStore";

const { apiGet, apiPost } = vi.hoisted(() => ({
	apiGet: vi.fn(),
	apiPost: vi.fn(),
}));

vi.mock("@/services/http", () => ({
	api: {
		get: apiGet,
		post: apiPost,
	},
}));

describe("taskService", () => {
	beforeEach(() => {
		apiGet.mockReset();
		apiPost.mockReset();
		useWorkspaceStore.getState().setWorkspace(PERSONAL_WORKSPACE);
	});

	it("uses the expected task endpoints for each workspace", () => {
		const personalTaskService = createTaskService(PERSONAL_WORKSPACE);

		personalTaskService.listInWorkspace({ limit: 20, offset: 40 });
		personalTaskService.getTask(7);
		personalTaskService.retryTask(7);

		const teamTaskService = createTaskService({ kind: "team", teamId: 5 });

		teamTaskService.listInWorkspace({ limit: 10 });
		teamTaskService.getTask(9);
		teamTaskService.retryTask(9);

		expect(apiGet).toHaveBeenNthCalledWith(1, "/tasks", {
			params: { limit: 20, offset: 40 },
		});
		expect(apiGet).toHaveBeenNthCalledWith(2, "/tasks/7");
		expect(apiPost).toHaveBeenNthCalledWith(1, "/tasks/7/retry");
		expect(apiGet).toHaveBeenNthCalledWith(3, "/teams/5/tasks", {
			params: { limit: 10 },
		});
		expect(apiGet).toHaveBeenNthCalledWith(4, "/teams/5/tasks/9");
		expect(apiPost).toHaveBeenNthCalledWith(2, "/teams/5/tasks/9/retry");
	});

	it("binds the shared task service to the current workspace for cached methods", () => {
		const listInWorkspace = taskService.listInWorkspace;
		const retryTask = taskService.retryTask;

		listInWorkspace({ offset: 2 });

		useWorkspaceStore.getState().setWorkspace({ kind: "team", teamId: 3 });

		listInWorkspace({ limit: 5 });
		retryTask(11);

		expect(apiGet).toHaveBeenNthCalledWith(1, "/tasks", {
			params: { offset: 2 },
		});
		expect(apiGet).toHaveBeenNthCalledWith(2, "/teams/3/tasks", {
			params: { limit: 5 },
		});
		expect(apiPost).toHaveBeenCalledWith("/teams/3/tasks/11/retry");
	});
	it("freezes a cancellation endpoint to the workspace selected at command creation", async () => {
		apiPost.mockResolvedValue({ status: "canceled" });
		const original = createTaskService(PERSONAL_WORKSPACE);
		useWorkspaceStore.getState().setWorkspace({ kind: "team", teamId: 42 });
		await original.cancelTask(7);
		await taskService.cancelTask(8);
		expect(apiPost).toHaveBeenNthCalledWith(1, "/tasks/7/cancel");
		expect(apiPost).toHaveBeenNthCalledWith(2, "/teams/42/tasks/8/cancel");
	});
});
