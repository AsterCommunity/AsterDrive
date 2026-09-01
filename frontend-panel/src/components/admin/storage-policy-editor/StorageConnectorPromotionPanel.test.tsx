import { fireEvent, render, screen, waitFor } from "@testing-library/react";
import type { ComponentProps } from "react";
import { describe, expect, it, vi } from "vitest";
import type { StorageConnectorDescriptor } from "@/types/api";
import type { StorageConnectorPromotionCandidate } from "./policyPromotion";
import { StorageConnectorPromotionPanel } from "./StorageConnectorPromotionPanel";
import type { Translate } from "./StoragePolicyFieldTypes";

vi.mock("@/lib/adminStorageConnectorLocalizations", () => ({
	translateStorageConnectorMessage: (
		t: Translate,
		_connectorId: string,
		key: string,
		values?: Record<string, number | string>,
	) => t(key, values),
}));

vi.mock("@/components/ui/icon", () => ({
	Icon: ({ name }: { name: string }) => <span>{name}</span>,
}));

const labels: Record<string, string> = {
	"core:cancel": "Cancel",
	policy_connector_promotion_action: "Promote to {{connector}}",
	policy_connector_promotion_confirm: "Promote connector",
	policy_connector_promotion_confirm_title: "Promote to {{connector}}?",
	policy_connector_promotion_title: "Specialized connector available",
	policy_connector_promotion_unsaved_blocked: "Save edits first.",
	policy_connector_promotion_use_draft: "Use {{connector}} instead",
	promotion_confirm: "Bucket and base path remain unchanged.",
	promotion_desc: "This endpoint matches the specialized connector.",
	target_label: "Tencent COS",
};

const t: Translate = (key, values) =>
	Object.entries(values ?? {}).reduce(
		(current, [name, value]) => current.replace(`{{${name}}}`, String(value)),
		labels[key] ?? key,
	);

const target = {
	connector_id: "asterdrive.storage.tencent_cos",
	ui: { label_key: "target_label" },
} as StorageConnectorDescriptor;

const candidate: StorageConnectorPromotionCandidate = {
	targetDescriptor: target,
	promotion: {
		config_mappings: [],
		confirmation_key: "promotion_confirm",
		description_key: "promotion_desc",
		promotion_id: "promote_from_s3",
		source_connector_id: "asterdrive.storage.s3",
	},
};

function renderPanel(
	overrides: Partial<
		ComponentProps<typeof StorageConnectorPromotionPanel>
	> = {},
) {
	const callbacks = {
		onApplyDraft: vi.fn(),
		onCancel: vi.fn(),
		onConfirm: vi.fn(),
		onRequest: vi.fn(),
	};
	const view = render(
		<StorageConnectorPromotionPanel
			blocked={false}
			candidates={[candidate]}
			confirmKey={null}
			mode="create"
			submittingKey={null}
			t={t}
			{...callbacks}
			{...overrides}
		/>,
	);
	return { ...callbacks, ...view };
}

describe("StorageConnectorPromotionPanel", () => {
	it("renders nothing without eligible promotions", () => {
		const { container } = renderPanel({ candidates: [] });
		expect(container).toBeEmptyDOMElement();
	});

	it("applies a create recommendation directly", () => {
		const { onApplyDraft } = renderPanel();
		fireEvent.click(
			screen.getByRole("button", { name: "Use Tencent COS instead" }),
		);
		expect(onApplyDraft).toHaveBeenCalledWith(candidate);
	});

	it("disables dirty saved promotions and explains the block", async () => {
		renderPanel({ blocked: true, mode: "edit" });
		expect(
			screen.getByRole("button", { name: "Promote to Tencent COS" }),
		).toBeDisabled();
		await waitFor(() =>
			expect(screen.getByText("Save edits first.")).toBeVisible(),
		);
	});

	it("renders connector-owned confirmation copy and dispatches controls", async () => {
		const key = `${target.connector_id}:promote_from_s3`;
		const { onCancel, onConfirm } = renderPanel({
			confirmKey: key,
			mode: "edit",
		});
		await waitFor(() =>
			expect(
				screen.getByText("Bucket and base path remain unchanged."),
			).toBeVisible(),
		);
		fireEvent.click(screen.getByRole("button", { name: "Cancel" }));
		fireEvent.click(screen.getByRole("button", { name: "Promote connector" }));
		expect(onCancel).toHaveBeenCalledTimes(1);
		expect(onConfirm).toHaveBeenCalledWith(candidate);
	});

	it("shows submitting state without allowing duplicate requests", async () => {
		const key = `${target.connector_id}:promote_from_s3`;
		renderPanel({ mode: "edit", submittingKey: key });
		expect(
			screen.getByRole("button", { name: /Promote to Tencent COS/ }),
		).toBeDisabled();
		await waitFor(() => expect(screen.getByText("Spinner")).toBeVisible());
	});

	it("keeps the last eligible content mounted while the panel collapses", () => {
		const view = renderPanel();
		const outerAnimation = view.container.querySelector(
			'[aria-hidden="false"]',
		);
		expect(outerAnimation).not.toBeNull();

		view.rerender(
			<StorageConnectorPromotionPanel
				blocked={false}
				candidates={[]}
				confirmKey={null}
				mode="create"
				submittingKey={null}
				t={t}
				onApplyDraft={view.onApplyDraft}
				onCancel={view.onCancel}
				onConfirm={view.onConfirm}
				onRequest={view.onRequest}
			/>,
		);

		expect(screen.getByText("Tencent COS")).toBeInTheDocument();
		expect(view.container.querySelector('[aria-hidden="true"]')).not.toBeNull();
	});
});
