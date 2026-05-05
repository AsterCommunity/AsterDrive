import { beforeEach, describe, expect, it, vi } from "vitest";
import { writeTextToClipboard } from "@/lib/clipboard";

const clipboardWriteText = vi.fn();
const execCommand = vi.fn();

function setNavigatorClipboard(writeText?: typeof clipboardWriteText) {
	Object.defineProperty(navigator, "clipboard", {
		configurable: true,
		value: writeText ? { writeText } : undefined,
	});
}

describe("writeTextToClipboard", () => {
	beforeEach(() => {
		clipboardWriteText.mockReset();
		execCommand.mockReset();
		setNavigatorClipboard(undefined);

		Object.defineProperty(document, "execCommand", {
			configurable: true,
			value: execCommand,
		});
	});

	it("uses the modern Clipboard API when available", async () => {
		clipboardWriteText.mockResolvedValue(undefined);
		setNavigatorClipboard(clipboardWriteText);

		await writeTextToClipboard("hello");

		expect(clipboardWriteText).toHaveBeenCalledWith("hello");
		expect(execCommand).not.toHaveBeenCalled();
	});

	it("falls back to the legacy copy command when Clipboard API rejects", async () => {
		clipboardWriteText.mockRejectedValue(new Error("denied"));
		execCommand.mockImplementation(() => {
			expect(document.querySelector("textarea")?.value).toBe("fallback");
			return true;
		});
		setNavigatorClipboard(clipboardWriteText);

		await writeTextToClipboard("fallback");

		expect(clipboardWriteText).toHaveBeenCalledWith("fallback");
		expect(execCommand).toHaveBeenCalledWith("copy");
		expect(document.querySelector("textarea")).toBeNull();
	});

	it("uses the legacy copy command when Clipboard API is unavailable", async () => {
		execCommand.mockReturnValue(true);

		await writeTextToClipboard("legacy");

		expect(execCommand).toHaveBeenCalledWith("copy");
	});

	it("rejects when every copy strategy fails", async () => {
		execCommand.mockReturnValue(false);

		await expect(writeTextToClipboard("fail")).rejects.toThrow(
			"Clipboard copy failed",
		);
	});
});
