import { describe, expect, it } from "vitest";
import {
	cn,
	folderTreeRowClass,
	SIDEBAR_SECTION_TITLE_CLASS,
	sidebarNavItemClass,
} from "@/lib/utils";

const expectClassTokens = (className: string, expectedTokens: string[]) => {
	expect(className.split(" ")).toEqual(expect.arrayContaining(expectedTokens));
};

describe("utils", () => {
	it("merges class names with tailwind conflict resolution", () => {
		expect(cn("px-2", false, undefined, "px-4", "text-sm")).toBe(
			"px-4 text-sm",
		);
	});

	it("builds active and inactive sidebar nav item classes", () => {
		expectClassTokens(sidebarNavItemClass(true, "custom"), [
			"select-none",
			"bg-accent",
			"text-accent-foreground",
			"font-medium",
			"custom",
		]);
		expectClassTokens(sidebarNavItemClass(false), [
			"select-none",
			"text-muted-foreground",
			"hover:bg-accent/45",
			"hover:text-foreground",
		]);
	});

	it("builds active and inactive folder tree row classes", () => {
		expectClassTokens(folderTreeRowClass(true, "custom"), [
			"bg-accent",
			"text-accent-foreground",
			"font-medium",
			"custom",
		]);
		expectClassTokens(folderTreeRowClass(false), ["text-foreground"]);
	});

	it("adds the D5 active indicator only when opted in and active", () => {
		const navWithIndicator = sidebarNavItemClass(true, undefined, {
			indicator: true,
		});
		expectClassTokens(navWithIndicator, [
			"relative",
			"before:absolute",
			"before:w-0.5",
			"before:rounded-full",
			"before:bg-primary",
		]);

		// 后台等未 opt-in 的调用方保持纯色块，不出现指示条
		expect(sidebarNavItemClass(true)).not.toContain("before:bg-primary");
		expect(sidebarNavItemClass(true)).not.toContain("relative");
		// 未选中时即使 opt-in 也不渲染指示条
		expect(
			sidebarNavItemClass(false, undefined, { indicator: true }),
		).not.toContain("before:bg-primary");

		expectClassTokens(
			folderTreeRowClass(true, undefined, { indicator: true }),
			["relative", "before:absolute", "before:bg-primary"],
		);
		expect(folderTreeRowClass(true)).not.toContain("before:bg-primary");
	});

	it("exposes the shared D5 sidebar section title class", () => {
		expectClassTokens(SIDEBAR_SECTION_TITLE_CLASS, [
			"text-[11px]",
			"font-medium",
			"uppercase",
			"tracking-wider",
		]);
	});
});
