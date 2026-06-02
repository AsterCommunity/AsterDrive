import { fireEvent, render, screen } from "@testing-library/react";
import { describe, expect, it, vi } from "vitest";
import { PreviewAppEditorFields } from "./PreviewAppEditorFields";
import type {
	PreviewAppsEditorApp,
	PreviewAppsEditorConfig,
} from "./previewAppsConfigEditorShared";

const t = (key: string) => key;

vi.mock("@/components/admin/DelimitedListInput", () => {
	function parseDelimitedListInputForTest(value: string) {
		const items: string[] = [];
		const seen = new Set<string>();
		for (const rawItem of value.split(",")) {
			const item = rawItem.trim();
			if (item.length === 0 || seen.has(item)) {
				continue;
			}
			seen.add(item);
			items.push(item);
		}
		return items;
	}

	return {
		DelimitedListInput: ({
			onValueChange,
			placeholder,
			values,
		}: {
			onValueChange: (values: string[]) => void;
			placeholder?: string;
			values: string[];
		}) => (
			<input
				aria-label={placeholder ?? "delimited-list"}
				value={values.join(", ")}
				onChange={(event) =>
					onValueChange(parseDelimitedListInputForTest(event.target.value))
				}
			/>
		),
	};
});

vi.mock("@/components/ui/input", () => ({
	Input: ({
		disabled,
		onChange,
		value,
	}: {
		disabled?: boolean;
		onChange?: (event: { target: { value: string } }) => void;
		value?: string;
	}) => (
		<input
			disabled={disabled}
			value={value ?? ""}
			onChange={(event) =>
				onChange?.({ target: { value: event.target.value } })
			}
		/>
	),
}));

vi.mock("@/components/ui/select", () => ({
	Select: ({
		children,
		items,
		onValueChange,
		value,
	}: {
		children: React.ReactNode;
		items?: Array<{ label: string; value: string }>;
		onValueChange?: (value: string) => void;
		value?: string;
	}) => (
		<div>
			<div>{`select:${value ?? ""}`}</div>
			{items?.map((item) => (
				<button
					key={item.value}
					type="button"
					aria-label={`select-item:${item.value}`}
					onClick={() => onValueChange?.(item.value)}
				>
					{item.label}
				</button>
			))}
			{children}
		</div>
	),
	SelectContent: ({ children }: { children: React.ReactNode }) => (
		<div>{children}</div>
	),
	SelectItem: ({
		children,
		value,
	}: {
		children: React.ReactNode;
		value: string;
	}) => <div data-value={value}>{children}</div>,
	SelectTrigger: ({
		"aria-label": ariaLabel,
		children,
	}: {
		"aria-label"?: string;
		children: React.ReactNode;
	}) => (
		<button type="button" aria-label={ariaLabel}>
			{children}
		</button>
	),
	SelectValue: () => <span>select-value</span>,
}));

function createUrlTemplateApp(
	overrides: Partial<PreviewAppsEditorApp> = {},
): PreviewAppsEditorApp {
	return {
		config: {
			allowed_origins: ["https://viewer.example.com"],
			mode: "iframe",
			url_template: "https://viewer.example.com/embed",
		},
		enabled: true,
		extensions: ["md"],
		icon: "https://viewer.example.com/icon.svg",
		key: "custom.viewer",
		labels: {
			en: "Viewer",
			zh: "查看器",
		},
		provider: "url_template",
		...overrides,
	};
}

function applyUpdateApp(
	updateApp: ReturnType<typeof vi.fn>,
	app: PreviewAppsEditorApp,
) {
	if (updateApp.mock.calls.length === 0) {
		throw new Error("applyUpdateApp: updateApp was not called");
	}
	const [, updater] = updateApp.mock.calls.at(-1) as [
		number,
		(app: PreviewAppsEditorApp) => PreviewAppsEditorApp,
	];
	return updater(app);
}

describe("PreviewAppEditorFields", () => {
	it("updates URL-template app identity, provider, matches, mode, and config fields", () => {
		const app = createUrlTemplateApp();
		const updateApp = vi.fn();
		const updateDraft = vi.fn();
		const onOpenUrlTemplateVariables = vi.fn();

		render(
			<PreviewAppEditorFields
				app={app}
				index={1}
				protectedBuiltin={false}
				t={t}
				updateApp={updateApp}
				updateDraft={updateDraft}
				onOpenUrlTemplateVariables={onOpenUrlTemplateVariables}
			/>,
		);

		expect(screen.getByText("preview_apps_provider_label")).toBeInTheDocument();
		expect(screen.getByText("select:url_template")).toBeInTheDocument();

		fireEvent.change(screen.getByDisplayValue("custom.viewer"), {
			target: { value: "custom.changed" },
		});
		const draftUpdater = updateDraft.mock.calls.at(-1)?.[0] as (
			current: PreviewAppsEditorConfig,
		) => PreviewAppsEditorConfig;
		expect(
			draftUpdater({
				apps: [createUrlTemplateApp(), app],
				version: 2,
			}).apps[1]?.key,
		).toBe("custom.changed");

		fireEvent.change(
			screen.getByDisplayValue("https://viewer.example.com/icon.svg"),
			{
				target: { value: "https://cdn.example.com/icon.svg" },
			},
		);
		const iconDraftUpdater = updateDraft.mock.calls.at(-1)?.[0] as (
			current: PreviewAppsEditorConfig,
		) => PreviewAppsEditorConfig;
		expect(
			iconDraftUpdater({
				apps: [createUrlTemplateApp(), app],
				version: 2,
			}).apps[1]?.icon,
		).toBe("https://cdn.example.com/icon.svg");

		fireEvent.click(screen.getByRole("button", { name: "select-item:wopi" }));
		expect(applyUpdateApp(updateApp, app)).toMatchObject({
			config: {
				mode: "iframe",
			},
			provider: "wopi",
		});

		fireEvent.change(
			screen.getByLabelText("preview_apps_matches_extensions_placeholder"),
			{
				target: { value: "md, markdown, txt" },
			},
		);
		expect(applyUpdateApp(updateApp, app).extensions).toEqual([
			"md",
			"markdown",
			"txt",
		]);

		fireEvent.click(
			screen.getByRole("button", { name: "select-item:new_tab" }),
		);
		expect(applyUpdateApp(updateApp, app).config.mode).toBe("new_tab");

		fireEvent.change(
			screen.getByDisplayValue("https://viewer.example.com/embed"),
			{
				target: { value: "https://viewer.example.com/open" },
			},
		);
		expect(applyUpdateApp(updateApp, app).config.url_template).toBe(
			"https://viewer.example.com/open",
		);

		fireEvent.change(screen.getByLabelText("delimited-list"), {
			target: { value: "https://viewer.example.com, https://cdn.example.com" },
		});
		expect(applyUpdateApp(updateApp, app).config.allowed_origins).toEqual([
			"https://viewer.example.com",
			"https://cdn.example.com",
		]);

		fireEvent.click(
			screen.getByRole("button", {
				name: "preview_apps_url_template_variables_link",
			}),
		);
		expect(onOpenUrlTemplateVariables).toHaveBeenCalledTimes(1);
	});

	it("locks protected builtin keys while still allowing labels and table delimiters", () => {
		const app = createUrlTemplateApp({
			config: { delimiter: "\t" },
			key: "builtin.table",
			provider: "builtin",
		});
		const updateApp = vi.fn();

		render(
			<PreviewAppEditorFields
				app={app}
				index={0}
				protectedBuiltin
				t={t}
				updateApp={updateApp}
				updateDraft={vi.fn()}
				onOpenUrlTemplateVariables={vi.fn()}
			/>,
		);

		expect(screen.getByDisplayValue("builtin.table")).toBeDisabled();
		expect(
			screen.getByText("preview_apps_builtin_key_locked"),
		).toBeInTheDocument();
		expect(
			screen.queryByText("preview_apps_provider_label"),
		).not.toBeInTheDocument();
		expect(screen.getByText("select:auto")).toBeInTheDocument();

		fireEvent.change(screen.getByDisplayValue("查看器"), {
			target: { value: "表格" },
		});
		expect(applyUpdateApp(updateApp, app).labels.zh).toBe("表格");

		fireEvent.change(screen.getByDisplayValue("Viewer"), {
			target: { value: "Table" },
		});
		expect(applyUpdateApp(updateApp, app).labels.en).toBe("Table");

		fireEvent.click(screen.getByRole("button", { name: "select-item:;" }));
		expect(applyUpdateApp(updateApp, app).config.delimiter).toBe(";");
	});

	it("renders WOPI app-specific fields and writes text config values", () => {
		const app = createUrlTemplateApp({
			config: {
				action_url: "https://office.example.com/wopi",
				discovery_url: "https://office.example.com/hosting/discovery",
				mode: "new_tab",
			},
			provider: "wopi",
		});
		const updateApp = vi.fn();

		render(
			<PreviewAppEditorFields
				app={app}
				index={0}
				protectedBuiltin={false}
				t={t}
				updateApp={updateApp}
				updateDraft={vi.fn()}
				onOpenUrlTemplateVariables={vi.fn()}
			/>,
		);

		expect(screen.getByText("preview_apps_wopi_hint_body")).toBeInTheDocument();
		expect(screen.getByText("select:new_tab")).toBeInTheDocument();

		fireEvent.change(
			screen.getByDisplayValue("https://office.example.com/wopi"),
			{
				target: { value: "https://office.example.com/new-wopi" },
			},
		);
		expect(applyUpdateApp(updateApp, app).config.action_url).toBe(
			"https://office.example.com/new-wopi",
		);

		fireEvent.change(
			screen.getByDisplayValue("https://office.example.com/hosting/discovery"),
			{
				target: { value: "https://office.example.com/new-discovery" },
			},
		);
		expect(applyUpdateApp(updateApp, app).config.discovery_url).toBe(
			"https://office.example.com/new-discovery",
		);
	});
});
