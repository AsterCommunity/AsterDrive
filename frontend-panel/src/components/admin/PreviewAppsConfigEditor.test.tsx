import { fireEvent, render, screen, within } from "@testing-library/react";
import { useState } from "react";
import { describe, expect, it, vi } from "vitest";
import { PreviewAppsConfigEditor } from "@/components/admin/PreviewAppsConfigEditor";
import { PREVIEW_APP_ICON_URLS } from "@/components/common/previewAppIconUrls";

vi.mock("react-i18next", () => ({
	useTranslation: () => ({
		i18n: { language: "zh-CN" },
		t: (key: string, values?: Record<string, number | string>) => {
			if (!values) {
				return key;
			}

			return Object.entries(values).reduce(
				(result, [name, value]) =>
					result.replaceAll(`{{${name}}}`, String(value)),
				key,
			);
		},
	}),
}));

function createPreviewAppsConfig() {
	return JSON.stringify(
		{
			version: 2,
			apps: [
				{
					extensions: ["md"],
					key: "custom.viewer",
					provider: "url_template",
					icon: "https://viewer.example.com/icon.svg",
					enabled: true,
					labels: {
						en: "Viewer",
						zh: "外部查看器",
					},
					config: {
						mode: "iframe",
						url_template:
							"https://viewer.example.com/embed?src={{file_preview_url}}",
						allowed_origins: ["https://viewer.example.com"],
					},
				},
			],
		},
		null,
		2,
	);
}

function createPreviewAppsConfigWithExtensions() {
	return JSON.stringify(
		{
			version: 2,
			apps: [
				{
					extensions: ["xlsx"],
					key: "custom.viewer",
					provider: "url_template",
					icon: "https://viewer.example.com/icon.svg",
					enabled: true,
					labels: {
						en: "Viewer",
						zh: "外部查看器",
					},
					config: {
						mode: "iframe",
						url_template:
							"https://viewer.example.com/embed?src={{file_preview_url}}",
						allowed_origins: ["https://viewer.example.com"],
					},
				},
			],
		},
		null,
		2,
	);
}

function createPreviewAppsConfigWithDefaultIcon() {
	return JSON.stringify(
		{
			version: 2,
			apps: [
				{
					key: "builtin.image",
					provider: "builtin",
					icon: PREVIEW_APP_ICON_URLS.image,
					enabled: true,
					labels: {
						zh: "图片预览",
					},
				},
			],
		},
		null,
		2,
	);
}

function StatefulPreviewAppsEditor({
	initialValue = createPreviewAppsConfig(),
}: {
	initialValue?: string;
}) {
	const [value, setValue] = useState(initialValue);
	return <PreviewAppsConfigEditor value={value} onChange={setValue} />;
}

function editAppByName(name: string) {
	const row = screen.getByText(name).closest("tr");
	expect(row).not.toBeNull();
	fireEvent.click(
		within(row as HTMLTableRowElement).getByRole("button", {
			name: "preview_apps_edit",
		}),
	);
}

describe("PreviewAppsConfigEditor", () => {
	it("opens the add dialog and creates an embed app", () => {
		render(<StatefulPreviewAppsEditor />);

		fireEvent.click(
			screen.getByRole("button", { name: "preview_apps_add_app" }),
		);

		expect(
			screen.getByText("preview_apps_add_dialog_desc"),
		).toBeInTheDocument();

		fireEvent.click(
			screen
				.getByText("preview_apps_add_dialog_embed_title")
				.closest("button") as HTMLButtonElement,
		);

		expect(screen.getByRole("dialog")).toBeInTheDocument();
		expect(screen.getByDisplayValue("custom.app_1")).toBeInTheDocument();
	});

	it("keeps the focused input active while typing in the app edit dialog", () => {
		render(<StatefulPreviewAppsEditor />);

		editAppByName("外部查看器");

		const keyInput = screen.getByDisplayValue("custom.viewer");
		keyInput.focus();
		expect(keyInput).toHaveFocus();

		fireEvent.change(keyInput, {
			target: { value: "custom.viewer.updated" },
		});

		expect(screen.getByDisplayValue("custom.viewer.updated")).toHaveFocus();
	});

	it("keeps delimited list text unformatted while the field is focused", () => {
		render(<StatefulPreviewAppsEditor />);

		editAppByName("外部查看器");

		const extensionInput = screen.getByDisplayValue("md");
		extensionInput.focus();
		fireEvent.change(extensionInput, {
			target: { value: "md,txt" },
		});

		expect(screen.getByDisplayValue("md,txt")).toHaveFocus();
		expect(screen.queryByDisplayValue("md, txt")).not.toBeInTheDocument();

		fireEvent.blur(extensionInput);

		expect(screen.getByDisplayValue("md, txt")).toBeInTheDocument();
	});

	it("opens the URL template magic variables dialog", () => {
		render(<StatefulPreviewAppsEditor />);

		editAppByName("外部查看器");
		fireEvent.click(
			screen.getByRole("button", {
				name: "preview_apps_url_template_variables_link",
			}),
		);

		expect(
			screen.getByText("preview_apps_url_template_variables_dialog_desc"),
		).toBeInTheDocument();
		expect(screen.getByText("{{download_path}}")).toBeInTheDocument();
		expect(screen.getByText("{{file_preview_url}}")).toBeInTheDocument();
	});

	it("shows an empty icon input when the configured icon matches the default", () => {
		render(
			<StatefulPreviewAppsEditor
				initialValue={createPreviewAppsConfigWithDefaultIcon()}
			/>,
		);

		editAppByName("图片预览");

		const dialog = screen.getByRole("dialog");
		const iconField = within(dialog)
			.getByText("preview_apps_icon_label")
			.parentElement?.querySelector("input");
		expect(iconField).not.toBeNull();
		expect(iconField).toHaveValue("");
	});

	it("opens the app editor in a dialog instead of inline content", () => {
		render(<StatefulPreviewAppsEditor />);

		editAppByName("外部查看器");

		const dialog = screen.getByRole("dialog");
		expect(
			within(dialog).getByDisplayValue("custom.viewer"),
		).toBeInTheDocument();
		expect(screen.getByText("preview_apps_dialog_desc")).toBeInTheDocument();
	});

	it("shows readable extension summaries before expanding an app", () => {
		render(
			<StatefulPreviewAppsEditor
				initialValue={createPreviewAppsConfigWithExtensions()}
			/>,
		);

		expect(screen.getByText(/xlsx/)).toBeInTheDocument();
		expect(screen.getAllByText("外部查看器").length).toBeGreaterThan(0);
		expect(
			screen.queryByRole("combobox", {
				name: "preview_apps_provider_label",
			}),
		).not.toBeInTheDocument();
	});
});
