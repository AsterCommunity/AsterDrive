import path from "node:path";
import tailwindcss from "@tailwindcss/vite";
import react from "@vitejs/plugin-react";
import { defineConfig } from "vite";
import { VitePWA } from "vite-plugin-pwa";

function getNodeModuleInfo(id: string) {
	const normalizedId = id.replaceAll("\\", "/");
	const nodeModulesSegment = "/node_modules/";
	const nodeModulesIndex = normalizedId.lastIndexOf(nodeModulesSegment);

	if (nodeModulesIndex === -1) return null;

	const modulePath = normalizedId.slice(
		nodeModulesIndex + nodeModulesSegment.length,
	);
	const [scopeOrName, maybeName, ...rest] = modulePath.split("/");

	if (!scopeOrName) return null;
	if (scopeOrName.startsWith("@")) {
		if (!maybeName) return null;
		return {
			packageName: `${scopeOrName}/${maybeName}`,
			subPath: rest.join("/"),
		};
	}

	return {
		packageName: scopeOrName,
		subPath: [maybeName, ...rest].filter(Boolean).join("/"),
	};
}

const BASE_UI_FORM_MODULES = new Set([
	"button",
	"checkbox",
	"checkbox-group",
	"field",
	"fieldset",
	"form",
	"input",
	"number-field",
	"radio",
	"radio-group",
	"toggle",
	"toggle-group",
]);

const BASE_UI_OVERLAY_MODULES = new Set([
	"alert-dialog",
	"autocomplete",
	"combobox",
	"context-menu",
	"dialog",
	"drawer",
	"floating-ui-react",
	"menu",
	"menubar",
	"popover",
	"preview-card",
	"select",
	"toast",
	"tooltip",
]);

const BASE_UI_CONTROL_MODULES = new Set([
	"accordion",
	"avatar",
	"collapsible",
	"composite",
	"meter",
	"navigation-menu",
	"progress",
	"scroll-area",
	"separator",
	"slider",
	"switch",
	"tabs",
	"toolbar",
]);

export default defineConfig(({ command }) => {
	const isDevServer = command === "serve";
	const rootReactPath = path.resolve(__dirname, "./node_modules/react");
	const rootReactDomPath = path.resolve(__dirname, "./node_modules/react-dom");

	return {
		plugins: [
			react(),
			tailwindcss(),
			VitePWA({
				registerType: "prompt",
				includeAssets: ["favicon.svg"],
				devOptions: {
					enabled: true,
					navigateFallbackAllowlist: [/^\/$/],
				},
				manifest: {
					name: "AsterDrive",
					short_name: "AsterDrive",
					description: "Self-hosted cloud storage",
					theme_color: "#0F172A",
					background_color: "#ffffff",
					display: "standalone",
					icons: [
						{
							src: "/favicon.svg",
							sizes: "any",
							type: "image/svg+xml",
							purpose: "any",
						},
						{
							src: "/favicon.svg",
							sizes: "any",
							type: "image/svg+xml",
							purpose: "maskable",
						},
					],
				},
				workbox: {
					globPatterns: isDevServer
						? []
						: ["**/*.{html,js,css,ico,png,svg,woff2,ttf,mjs,bcmap}"],
					globIgnores: isDevServer
						? []
						: [
								"assets/PdfPreview-*.js",
								"assets/PdfPreview-*.css",
								"assets/pdf.worker.min-*.mjs",
							],
					navigateFallback: "index.html",
					navigateFallbackDenylist: [/^\/api\//, /^\/health\//, /^\/d\//],
					runtimeCaching: [
						{
							urlPattern: ({ request, url }) =>
								url.pathname.startsWith("/assets/") &&
								(request.destination === "script" ||
									request.destination === "style" ||
									request.destination === "font" ||
									request.destination === "worker"),
							handler: "StaleWhileRevalidate",
							options: {
								cacheName: "asset-chunks",
								expiration: {
									maxEntries: 128,
									maxAgeSeconds: 60 * 60 * 24 * 30,
								},
							},
						},
					],
				},
			}),
		],
		base: "/",
		resolve: {
			alias: [
				{ find: "@", replacement: path.resolve(__dirname, "./src") },
				{
					find: "react/jsx-dev-runtime",
					replacement: path.resolve(rootReactPath, "./jsx-dev-runtime.js"),
				},
				{
					find: "react/jsx-runtime",
					replacement: path.resolve(rootReactPath, "./jsx-runtime.js"),
				},
				{ find: "react-dom", replacement: rootReactDomPath },
				{ find: "react", replacement: rootReactPath },
			],
			dedupe: ["react", "react-dom"],
		},
		server: {
			proxy: {
				"/api": "http://127.0.0.1:3000",
				"/health": "http://127.0.0.1:3000",
			},
		},
		build: {
			target: "esnext",
			modulePreload: false,
			outDir: "dist",
			emptyOutDir: true,
			rollupOptions: {
				output: {
					manualChunks(id) {
						const moduleInfo = getNodeModuleInfo(id);
						if (!moduleInfo) return;

						const { packageName, subPath } = moduleInfo;
						const baseUiModule = subPath.split("/")[0];

						if (
							packageName === "react" ||
							packageName === "react-dom" ||
							packageName === "scheduler"
						) {
							return "vendor-react";
						}

						if (
							packageName === "react-router" ||
							packageName === "react-router-dom"
						) {
							return "vendor-router";
						}

						if (packageName === "@base-ui/react") {
							if (BASE_UI_FORM_MODULES.has(baseUiModule)) {
								return "vendor-ui-forms";
							}
							if (BASE_UI_OVERLAY_MODULES.has(baseUiModule)) {
								return "vendor-ui-overlays";
							}
							if (BASE_UI_CONTROL_MODULES.has(baseUiModule)) {
								return "vendor-ui-controls";
							}
							return "vendor-ui-base";
						}

						if (packageName === "@floating-ui/react-dom") {
							return "vendor-ui-overlays";
						}

						if (packageName === "i18next" || packageName === "react-i18next") {
							return "vendor-i18n";
						}

						if (packageName === "react-icons") {
							return "vendor-react-icons";
						}

						if (
							packageName === "@devicon/react" ||
							packageName === "react-devicons"
						) {
							return "vendor-devicons";
						}

						if (packageName === "papaparse") {
							return "preview-data";
						}

						if (packageName === "xml-formatter") {
							return "preview-xml";
						}
					},
				},
			},
		},
		test: {
			include: ["src/**/*.test.ts", "src/**/*.test.tsx"],
			exclude: ["e2e/**", "node_modules/**", "dist/**"],
			environment: "jsdom",
			setupFiles: "./src/test/setup.ts",
			restoreMocks: true,
			coverage: {
				provider: "v8",
				include: ["src/**/*.{ts,tsx}"],
				exclude: [
					"src/**/*.test.{ts,tsx}",
					"src/test/**",
					"src/services/api.generated.ts",
					"src/types/**/*.d.ts",
				],
				reporter: ["text", "json-summary", "lcov", "html"],
				reportsDirectory: "./coverage",
				reportOnFailure: true,
			},
			server: {
				deps: {
					inline: [/^react-devicons(?:\/|$)/],
				},
			},
		},
	};
});
