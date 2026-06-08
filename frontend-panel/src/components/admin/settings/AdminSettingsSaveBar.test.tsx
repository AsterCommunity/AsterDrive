import { fireEvent, render, screen } from "@testing-library/react";
import { describe, expect, it, vi } from "vitest";
import { AdminSettingsSaveBar } from "@/components/admin/settings/AdminSettingsSaveBar";

vi.mock("react-i18next", () => ({
	useTranslation: () => ({
		t: (key: string, options?: Record<string, unknown>) => {
			if (key === "settings_save_notice") {
				return `settings_save_notice:${options?.count}`;
			}
			return key;
		},
	}),
}));

vi.mock("@/components/ui/button", () => ({
	Button: ({
		children,
		disabled,
		onClick,
		...props
	}: {
		children: React.ReactNode;
		disabled?: boolean;
		onClick?: () => void;
		[key: string]: unknown;
	}) => (
		<button disabled={disabled} onClick={onClick} {...props}>
			{children}
		</button>
	),
}));

function createProps(
	overrides: Partial<React.ComponentProps<typeof AdminSettingsSaveBar>> = {},
): React.ComponentProps<typeof AdminSettingsSaveBar> {
	return {
		changedCount: 2,
		hasUnsavedChanges: true,
		hasValidationError: false,
		measureRef: { current: document.createElement("div") },
		phase: "visible",
		saving: false,
		validationMessage: undefined,
		onDiscardChanges: vi.fn(),
		onSaveAll: vi.fn(),
		...overrides,
	};
}

describe("AdminSettingsSaveBar", () => {
	it("does not render while the bar is hidden", () => {
		render(<AdminSettingsSaveBar {...createProps({ phase: "hidden" })} />);

		expect(screen.queryByTestId("settings-save-bar")).not.toBeInTheDocument();
	});

	it("shows the shared notice and wires discard/save actions when changes are valid", () => {
		const onDiscardChanges = vi.fn();
		const onSaveAll = vi.fn();

		render(
			<AdminSettingsSaveBar
				{...createProps({
					changedCount: 3,
					onDiscardChanges,
					onSaveAll,
				})}
			/>,
		);

		expect(screen.getByText("settings_save_notice:3")).toBeInTheDocument();
		expect(screen.getByTestId("settings-save-bar")).toHaveClass(
			"z-(--z-fixed)",
		);
		expect(screen.getByText("settings_save_hint")).toBeInTheDocument();

		fireEvent.click(screen.getByRole("button", { name: "undo_changes" }));
		fireEvent.click(screen.getByRole("button", { name: "save_changes" }));

		expect(onDiscardChanges).toHaveBeenCalledTimes(1);
		expect(onSaveAll).toHaveBeenCalledTimes(1);
	});

	it("disables saving and surfaces the validation message when the draft is invalid", () => {
		render(
			<AdminSettingsSaveBar
				{...createProps({
					hasValidationError: true,
					validationMessage: "Duplicate custom config key",
				})}
			/>,
		);

		expect(screen.getByText("Duplicate custom config key")).toBeInTheDocument();
		expect(screen.getByRole("button", { name: "save_changes" })).toBeDisabled();
	});
});
