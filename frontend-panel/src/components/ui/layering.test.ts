import { readFileSync } from "node:fs";
import { resolve } from "node:path";
import { describe, expect, it } from "vitest";

function readProjectFile(path: string) {
	return readFileSync(resolve(process.cwd(), path), "utf8");
}

function readZToken(css: string, token: string) {
	const match = css.match(new RegExp(`${token}:\\s*(\\d+);`));

	if (!match) {
		throw new Error(`Missing z-index token ${token}`);
	}

	return Number(match[1]);
}

describe("overlay layering", () => {
	it("keeps shared overlay z-index tokens in the expected stack order", () => {
		const css = readProjectFile("src/index.css");
		const fixed = readZToken(css, "--z-fixed");
		const dialog = readZToken(css, "--z-dialog");
		const dropdown = readZToken(css, "--z-dropdown");
		const popover = readZToken(css, "--z-popover");
		const tooltip = readZToken(css, "--z-tooltip");
		const alertDialog = readZToken(css, "--z-alert-dialog");
		const toast = readZToken(css, "--z-toast");

		expect(dialog).toBeGreaterThan(fixed);
		expect(dropdown).toBeGreaterThan(dialog);
		expect(popover).toBeGreaterThan(dialog);
		expect(tooltip).toBeGreaterThan(popover);
		expect(alertDialog).toBeGreaterThan(tooltip);
		expect(toast).toBeGreaterThan(alertDialog);
	});

	it("uses semantic z-index tokens in the shared overlay components", () => {
		expect(readProjectFile("src/components/ui/dialog.tsx")).toContain(
			"z-(--z-dialog)",
		);
		expect(readProjectFile("src/components/ui/alert-dialog.tsx")).toContain(
			"z-(--z-alert-dialog)",
		);
		expect(readProjectFile("src/components/ui/dropdown-menu.tsx")).toContain(
			"z-(--z-dropdown)",
		);
		expect(readProjectFile("src/components/ui/context-menu.tsx")).toContain(
			"z-(--z-dropdown)",
		);
		expect(readProjectFile("src/components/ui/select.tsx")).toContain(
			"z-(--z-popover)",
		);
		expect(readProjectFile("src/components/ui/tooltip.tsx")).toContain(
			"z-(--z-tooltip)",
		);
		expect(readProjectFile("src/App.tsx")).toContain(
			'style={{ zIndex: "var(--z-toast)" }}',
		);
	});

	it("keeps fixed chrome below dialog overlays", () => {
		for (const path of [
			"src/components/common/BatchActionBar.tsx",
			"src/pages/file-browser/FileBrowserToolbar.tsx",
			"src/components/trash/TrashBatchActionBar.tsx",
			"src/pages/my-shares/MySharesSelectionBar.tsx",
			"src/components/layout/AdminLayout.tsx",
			"src/components/layout/Sidebar.tsx",
			"src/components/files/UploadPanel.tsx",
			"src/components/files/UploadArea.tsx",
			"src/components/music/MusicPlayerHost.tsx",
			"src/components/admin/settings/AdminSettingsSaveBar.tsx",
		]) {
			expect(readProjectFile(path)).toContain("z-(--z-fixed)");
		}
	});
});
