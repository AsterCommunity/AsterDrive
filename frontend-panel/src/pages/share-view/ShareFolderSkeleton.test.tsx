import { render } from "@testing-library/react";
import { describe, expect, it, vi } from "vitest";
import {
	ShareFolderContentSkeleton,
	ShareLoadingSkeleton,
} from "./ShareFolderSkeleton";

vi.mock("@/components/layout/ShareTopBar", () => ({
	ShareTopBar: () => <div data-testid="share-topbar" />,
}));

describe("ShareFolderSkeleton", () => {
	it("matches the folder content grid dimensions", () => {
		const { container } = render(
			<ShareFolderContentSkeleton viewMode="grid" />,
		);

		expect(container.querySelector('[data-slot="skeleton"]')).toHaveClass(
			"h-20",
		);
		expect(container.querySelector(".min-h-\\[166px\\]")).toBeInTheDocument();
	});

	it("uses the read-only four-column table shape", () => {
		const { container } = render(
			<ShareFolderContentSkeleton viewMode="list" />,
		);

		expect(container.querySelectorAll('[data-slot="table-head"]')).toHaveLength(
			4,
		);
		expect(container.querySelectorAll('[data-slot="table-row"]')).toHaveLength(
			9,
		);
	});

	it("uses the same toolbar and scroll structure as the folder page", () => {
		const { container } = render(<ShareLoadingSkeleton />);

		expect(container.querySelector("main")).toHaveClass(
			"flex",
			"flex-col",
			"overflow-hidden",
		);
		expect(container.querySelector("section")).toHaveClass(
			"min-h-0",
			"flex-1",
			"overflow-auto",
		);
		expect(
			container.querySelector('[data-testid="share-topbar"]'),
		).toBeInTheDocument();
		expect(container.querySelector(".max-w-7xl")).not.toBeInTheDocument();
	});
});
