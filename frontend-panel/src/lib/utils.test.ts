import { describe, expect, it } from "vitest";
import { cn, folderTreeRowClass, sidebarNavItemClass } from "@/lib/utils";

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
});
