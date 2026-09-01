import { fireEvent, render, screen } from "@testing-library/react";
import { describe, expect, it, vi } from "vitest";
import { StoragePolicyEditorActions } from "./StoragePolicyEditorActions";

vi.mock("react-i18next", () => ({
	useTranslation: () => ({
		t: (key: string) => key,
	}),
}));

vi.mock("@/components/ui/icon", () => ({
	Icon: ({ name }: { name: string }) => <span>{name}</span>,
}));

function descriptor(overrides: Record<string, unknown> = {}) {
	return {
		actions: [],
		fields: [],
		ui: {},
		connector_id: "plugin.example",
		...overrides,
	} as never;
}

function renderActions(
	overrides: Partial<ComponentProps<typeof StoragePolicyEditorActions>> = {},
) {
	const handlers = {
		onBack: vi.fn(),
		onCancel: vi.fn(),
		onRunConnectionTest: vi.fn(async () => true),
	};
	render(
		<StoragePolicyEditorActions
			mode="edit"
			createStep={0}
			submitting={false}
			descriptor={descriptor()}
			{...handlers}
			{...overrides}
		/>,
	);
	return handlers;
}

import type { ComponentProps } from "react";

describe("StoragePolicyEditorActions", () => {
	it("shows cancel and save for the edit mode", () => {
		const handlers = renderActions();

		fireEvent.click(screen.getByRole("button", { name: "core:cancel" }));
		expect(handlers.onCancel).toHaveBeenCalledOnce();
		expect(screen.getByRole("button", { name: /save_changes/ })).toBeEnabled();
	});

	it("hides the cancel button when no onCancel is provided (setup shell)", () => {
		renderActions({ onCancel: undefined });

		expect(
			screen.queryByRole("button", { name: "core:cancel" }),
		).not.toBeInTheDocument();
		expect(
			screen.getByRole("button", { name: /save_changes/ }),
		).toBeInTheDocument();
	});

	it("shows only the cancel button on the connector selection step", () => {
		renderActions({ mode: "create", createStep: 0 });

		expect(
			screen.getByRole("button", { name: "core:cancel" }),
		).toBeInTheDocument();
		expect(
			screen.queryByRole("button", { name: "policy_wizard_next" }),
		).not.toBeInTheDocument();
		expect(
			screen.queryByRole("button", { name: "core:create" }),
		).not.toBeInTheDocument();
	});

	it("shows back and review on the configuration step and routes back", () => {
		const handlers = renderActions({ mode: "create", createStep: 1 });

		fireEvent.click(screen.getByRole("button", { name: "core:back" }));
		expect(handlers.onBack).toHaveBeenCalledOnce();
		expect(
			screen.getByRole("button", { name: "policy_wizard_review" }),
		).toBeInTheDocument();
	});

	it("shows the create action on the review step", () => {
		renderActions({ mode: "create", createStep: 2 });

		expect(
			screen.getByRole("button", { name: "core:create" }),
		).toBeInTheDocument();
	});

	it("disables the primary action while submitting or without a descriptor", () => {
		const { unmount } = render(
			<StoragePolicyEditorActions
				mode="edit"
				createStep={0}
				submitting
				descriptor={descriptor()}
				onBack={vi.fn()}
				onCancel={vi.fn()}
				onRunConnectionTest={vi.fn(async () => true)}
			/>,
		);
		expect(screen.getByRole("button", { name: /save_changes/ })).toBeDisabled();
		unmount();

		renderActions({ descriptor: null });
		expect(screen.getByRole("button", { name: /save_changes/ })).toBeDisabled();
	});

	it("runs the connection test when the descriptor supports it", async () => {
		const handlers = renderActions({
			descriptor: descriptor({
				actions: [
					{
						action_id: "test_saved_connection",
						kind: "connection_test",
						endpoints: ["test_policy_connection"],
						requires_saved_policy: true,
					},
				],
			}),
		});

		fireEvent.click(screen.getByRole("button", { name: /test_connection/ }));
		await vi.waitFor(() =>
			expect(handlers.onRunConnectionTest).toHaveBeenCalledOnce(),
		);
	});

	it("hides the connection test when the descriptor lacks support", () => {
		renderActions();

		expect(
			screen.queryByRole("button", { name: /test_connection/ }),
		).not.toBeInTheDocument();
	});
});
