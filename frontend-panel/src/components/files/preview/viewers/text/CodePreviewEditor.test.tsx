import { fireEvent, render, screen } from "@testing-library/react";
import { describe, expect, it, vi } from "vitest";
import { CodePreviewEditor } from "@/components/files/preview/viewers/text/CodePreviewEditor";

vi.mock("react-i18next", () => ({
	useTranslation: () => ({
		t: (key: string) => (key === "code_editor" ? "Code editor" : key),
	}),
}));

describe("CodePreviewEditor", () => {
	it("disables textarea soft wrapping when wordWrap is off", () => {
		render(
			<CodePreviewEditor
				language="plaintext"
				theme="vs"
				value={"const url = 'https://example.com/really/long/path';"}
				options={{
					readOnly: false,
					wordWrap: "off",
				}}
			/>,
		);

		expect(screen.getByLabelText("Code editor")).toHaveAttribute("wrap", "off");
	});

	it("keeps preformatted lines unwrapped in read-only mode", () => {
		const { container } = render(
			<CodePreviewEditor
				language="plaintext"
				theme="vs"
				value={"const url = 'https://example.com/really/long/path';"}
				options={{
					readOnly: true,
					wordWrap: "off",
				}}
			/>,
		);

		expect(container.querySelector("pre")).toHaveStyle({ whiteSpace: "pre" });
	});

	it("does not intercept textarea keys while IME composition is active", () => {
		const onChange = vi.fn();
		render(
			<CodePreviewEditor
				language="plaintext"
				theme="vs"
				value="hello"
				onChange={onChange}
				options={{
					readOnly: false,
				}}
			/>,
		);

		const textarea = screen.getByLabelText(
			"Code editor",
		) as HTMLTextAreaElement;
		textarea.setSelectionRange(2, 2);

		fireEvent.compositionStart(textarea);
		fireEvent.keyDown(textarea, { key: "Tab" });

		expect(onChange).not.toHaveBeenCalled();
	});

	it("treats keyCode 229 as IME composition for iPad Safari", () => {
		const onChange = vi.fn();
		render(
			<CodePreviewEditor
				language="plaintext"
				theme="vs"
				value="hello"
				onChange={onChange}
				options={{
					readOnly: false,
				}}
			/>,
		);

		const textarea = screen.getByLabelText(
			"Code editor",
		) as HTMLTextAreaElement;
		textarea.setSelectionRange(2, 2);

		fireEvent.keyDown(textarea, { key: "Tab", keyCode: 229 });

		expect(onChange).not.toHaveBeenCalled();
	});
});
