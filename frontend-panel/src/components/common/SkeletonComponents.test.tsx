import { render } from "@testing-library/react";
import { describe, expect, it } from "vitest";
import { SkeletonCard } from "@/components/common/SkeletonCard";
import { SkeletonFileGrid } from "@/components/common/SkeletonFileGrid";
import { SkeletonFileTable } from "@/components/common/SkeletonFileTable";
import { SkeletonTable } from "@/components/common/SkeletonTable";
import { SkeletonTree } from "@/components/common/SkeletonTree";
import {
	FOLDER_TREE_INDENT_PX,
	FOLDER_TREE_SKELETON_OFFSET_PX,
} from "@/lib/constants";

function countSkeletons(container: HTMLElement) {
	return container.querySelectorAll('[data-slot="skeleton"]').length;
}

describe("skeleton components", () => {
	it("renders the configured number of card placeholders", () => {
		const { container } = render(<SkeletonCard itemCount={3} />);

		expect(countSkeletons(container)).toBe(9);
	});

	it("renders the configured number of file grid placeholders", () => {
		const { container } = render(<SkeletonFileGrid count={4} />);

		expect(countSkeletons(container)).toBe(16);
	});

	it("renders the configured number of file table placeholders", () => {
		const { container } = render(<SkeletonFileTable rows={2} />);

		expect(countSkeletons(container)).toBe(11);
		expect(container.querySelectorAll('[data-slot="table-row"]')).toHaveLength(
			3,
		);
	});

	it("renders a generic table skeleton for the requested rows and columns", () => {
		const { container } = render(<SkeletonTable columns={3} rows={2} />);

		expect(countSkeletons(container)).toBe(9);
		expect(container.querySelectorAll('[data-slot="table-head"]')).toHaveLength(
			3,
		);
		expect(container.querySelectorAll('[data-slot="table-cell"]')).toHaveLength(
			6,
		);
	});

	it("renders a tree skeleton with indented placeholder rows", () => {
		const { container } = render(<SkeletonTree count={4} />);

		const rows = Array.from(
			container.firstChild?.childNodes ?? [],
		) as HTMLElement[];

		expect(countSkeletons(container)).toBe(12);
		expect(rows).toHaveLength(4);
		expect(rows[0]).toHaveStyle({
			paddingLeft: `${FOLDER_TREE_SKELETON_OFFSET_PX}px`,
		});
		expect(rows[1]).toHaveStyle({
			paddingLeft: `${FOLDER_TREE_INDENT_PX + FOLDER_TREE_SKELETON_OFFSET_PX}px`,
		});
	});
});
