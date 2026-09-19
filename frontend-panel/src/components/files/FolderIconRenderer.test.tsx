import { render, screen } from "@testing-library/react";
import { describe, expect, it } from "vitest";
import { FolderGlyph } from "./FolderGlyph";
import { FolderIconRenderer } from "./FolderIconRenderer";
import { folderBuiltinIconKeys, folderIconCatalog } from "./folderIconCatalog";

describe("FolderIconRenderer", () => {
	it("renders default, every generated builtin key, and native emoji", () => {
		const defaultIcon = <span>surface-default</span>;
		const { container, rerender } = render(
			<FolderIconRenderer defaultIcon={defaultIcon} />,
		);
		expect(screen.getByText("surface-default")).toBeInTheDocument();
		expect(container.querySelector("svg")).toBeNull();

		for (const key of folderBuiltinIconKeys) {
			rerender(
				<FolderIconRenderer
					icon={{ kind: "builtin", key }}
					defaultIcon={defaultIcon}
				/>,
			);
			expect(container.querySelector("svg"), key).toBeTruthy();
			expect(folderIconCatalog[key]).toBeDefined();
		}

		rerender(
			<FolderIconRenderer
				icon={{ kind: "emoji", value: "📚" }}
				defaultIcon={defaultIcon}
			/>,
		);
		expect(screen.getByText("📚")).toBeInTheDocument();
		expect(container.querySelector("svg")).toBeNull();
	});

	it("forwards icon and class names through the grid glyph wrapper", () => {
		const { container, rerender } = render(<FolderGlyph />);
		expect(container.querySelector("svg")).toBeInTheDocument();

		rerender(
			<FolderGlyph icon={{ kind: "emoji", value: "🗄️" }} className="size-12" />,
		);
		expect(screen.getByText("🗄️")).toHaveClass("size-12");
	});
});
