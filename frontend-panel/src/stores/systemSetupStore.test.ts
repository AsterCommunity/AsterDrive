import { HttpResponse, http } from "msw";
import { beforeEach, describe, expect, it } from "vitest";
import { apiResponse } from "@/test/fixtures";
import { server } from "@/test/server";

async function loadStore() {
	const { useSystemSetupStore } = await import("./systemSetupStore");
	useSystemSetupStore.setState({
		error: null,
		isChecking: false,
		setupState: null,
	});
	return useSystemSetupStore;
}

describe("useSystemSetupStore", () => {
	beforeEach(() => {
		server.resetHandlers();
	});

	it("loads the authoritative setup state", async () => {
		server.use(
			http.post("*/api/v1/auth/check", () =>
				HttpResponse.json(
					apiResponse({
						allow_user_registration: false,
						has_users: true,
						passkey_login_enabled: true,
						setup_state: "needs_storage",
					}),
				),
			),
		);
		const store = await loadStore();

		await expect(store.getState().refresh()).resolves.toBe("needs_storage");
		expect(store.getState()).toMatchObject({
			error: null,
			isChecking: false,
			setupState: "needs_storage",
		});
	});

	it("deduplicates concurrent setup checks", async () => {
		let requestCount = 0;
		server.use(
			http.post("*/api/v1/auth/check", async () => {
				requestCount += 1;
				await new Promise((resolve) => setTimeout(resolve, 20));
				return HttpResponse.json(
					apiResponse({
						allow_user_registration: false,
						has_users: true,
						passkey_login_enabled: true,
						setup_state: "ready",
					}),
				);
			}),
		);
		const store = await loadStore();

		const first = store.getState().refresh();
		const second = store.getState().refresh();
		await expect(Promise.all([first, second])).resolves.toEqual([
			"ready",
			"ready",
		]);
		expect(requestCount).toBe(1);
	});

	it("keeps the previous state and exposes refresh failures", async () => {
		server.use(http.post("*/api/v1/auth/check", () => HttpResponse.error()));
		const store = await loadStore();
		store.getState().setSetupState("needs_storage");

		await expect(store.getState().refresh()).rejects.toBeDefined();
		expect(store.getState().setupState).toBe("needs_storage");
		expect(store.getState().error).toBeDefined();
		expect(store.getState().isChecking).toBe(false);
	});

	it("clears a failed request before retrying and accepts the recovered state", async () => {
		let requestCount = 0;
		server.use(
			http.post("*/api/v1/auth/check", () => {
				requestCount += 1;
				if (requestCount === 1) return HttpResponse.error();
				return HttpResponse.json(
					apiResponse({
						allow_user_registration: false,
						has_users: true,
						passkey_login_enabled: true,
						setup_state: "ready",
					}),
				);
			}),
		);
		const store = await loadStore();

		await expect(store.getState().refresh()).rejects.toBeDefined();
		expect(store.getState().error).toBeDefined();

		await expect(store.getState().refresh()).resolves.toBe("ready");
		expect(requestCount).toBe(2);
		expect(store.getState()).toMatchObject({
			error: null,
			isChecking: false,
			setupState: "ready",
		});
	});

	it("lets an authenticated flow replace stale error state immediately", async () => {
		server.use(http.post("*/api/v1/auth/check", () => HttpResponse.error()));
		const store = await loadStore();

		await expect(store.getState().refresh()).rejects.toBeDefined();
		store.getState().setSetupState("needs_storage");

		expect(store.getState()).toMatchObject({
			error: null,
			isChecking: false,
			setupState: "needs_storage",
		});
	});
});
