import { render, screen } from "@testing-library/react";
import { describe, expect, it } from "vitest";
import { FolderIconRenderer } from "./FolderIconRenderer";
import { folderBuiltinIconKeys, folderIconCatalog } from "./folderIconCatalog";

describe("FolderIconRenderer", () => {
	it("renders default, every generated builtin key, and native emoji", () => {
		const { container, rerender } = render(<FolderIconRenderer />);
		expect(container.querySelector("svg")).toBeTruthy();

		for (const key of folderBuiltinIconKeys) {
			rerender(<FolderIconRenderer icon={{ kind: "builtin", key }} />);
			expect(container.querySelector("svg"), key).toBeTruthy();
			expect(folderIconCatalog[key]).toBeDefined();
		}

		rerender(<FolderIconRenderer icon={{ kind: "emoji", value: "📚" }} />);
		expect(screen.getByText("📚")).toBeInTheDocument();
		expect(container.querySelector("svg")).toBeNull();
	});
});
