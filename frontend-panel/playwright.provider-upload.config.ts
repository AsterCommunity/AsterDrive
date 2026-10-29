import { defineConfig } from "@playwright/test";

// Browser protocol tests import the real service adapter through Vite. They use
// an isolated cross-origin HTTP peer and need neither Drive credentials nor DB.
export default defineConfig({
	testDir: "./protocol-e2e",
	workers: 1,
	timeout: 30_000,
	use: { baseURL: "http://127.0.0.1:3347", serviceWorkers: "block" },
	webServer: {
		command: "bunx vite --host 127.0.0.1 --port 3347 --strictPort",
		url: "http://127.0.0.1:3347/protocol-e2e/fixtures/provider-upload.html",
		reuseExistingServer: false,
	},
});
