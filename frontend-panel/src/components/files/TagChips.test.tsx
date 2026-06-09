import { fireEvent, render, screen } from "@testing-library/react";
import { describe, expect, it, vi } from "vitest";
import { TagChip, TagChips } from "@/components/files/TagChips";
import { safeTagColor, tagColorFromName } from "@/components/files/tagColors";

vi.mock("@/components/ui/icon", () => ({
	Icon: ({ name }: { name: string }) => <span>{name}</span>,
}));

describe("TagChips", () => {
	it("accepts only full hex colors and falls back for invalid values", () => {
		expect(safeTagColor("#abcdef")).toBe("#abcdef");
		expect(safeTagColor("#ABCDEF")).toBe("#ABCDEF");
		expect(safeTagColor("#12345")).toBe("#64748b");
		expect(safeTagColor("#12345g")).toBe("#64748b");
		expect(safeTagColor("123456")).toBe("#64748b");
		expect(safeTagColor(null)).toBe("#64748b");
		expect(safeTagColor(undefined)).toBe("#64748b");
	});

	it("generates stable colors from trimmed case-insensitive names", () => {
		expect(tagColorFromName(" Important ")).toBe(tagColorFromName("important"));
		expect(tagColorFromName("")).toBe("#2563eb");
		expect(tagColorFromName("   ")).toBe("#2563eb");
		expect(tagColorFromName(null)).toBe("#2563eb");
		expect(tagColorFromName(undefined)).toBe("#2563eb");
	});

	it("renders empty fallback when no tags are available", () => {
		const { rerender } = render(
			<TagChips tags={[]} empty={<span>No tags</span>} />,
		);

		expect(screen.getByText("No tags")).toBeInTheDocument();

		rerender(<TagChips tags={null} empty={<span>No tags</span>} />);
		expect(screen.getByText("No tags")).toBeInTheDocument();
	});

	it("limits visible tags and shows a hidden count", () => {
		render(
			<TagChips
				maxVisible={2}
				tags={[
					{ id: 1, name: "Alpha", color: "#2563eb" },
					{ id: 2, name: "Beta", color: "#16a34a" },
					{ id: 3, name: "Gamma", color: "#dc2626" },
					{ id: 4, name: "Delta", color: "#0891b2" },
				]}
			/>,
		);

		expect(screen.getByText("Alpha")).toBeInTheDocument();
		expect(screen.getByText("Beta")).toBeInTheDocument();
		expect(screen.queryByText("Gamma")).not.toBeInTheDocument();
		expect(screen.getByText("+2")).toBeInTheDocument();
	});

	it("normalizes invalid maxVisible values to hide every chip behind the count", () => {
		render(
			<TagChips
				maxVisible={Number.NaN}
				tags={[
					{ id: 1, name: "Alpha", color: "#2563eb" },
					{ id: 2, name: "Beta", color: "#16a34a" },
				]}
			/>,
		);

		expect(screen.queryByText("Alpha")).not.toBeInTheDocument();
		expect(screen.queryByText("Beta")).not.toBeInTheDocument();
		expect(screen.getByText("+2")).toBeInTheDocument();
	});

	it("renders removable chips with an accessible remove action", () => {
		const onRemove = vi.fn();

		render(
			<TagChip
				tag={{ id: 1, name: "Important", color: "not-a-color" }}
				removeLabel="Remove Important"
				onRemove={onRemove}
			/>,
		);

		fireEvent.click(screen.getByRole("button", { name: "Remove Important" }));

		expect(onRemove).toHaveBeenCalledTimes(1);
		expect(screen.getByTitle("Important")).toBeInTheDocument();
	});
});
