import { fireEvent, render, screen, waitFor } from "@testing-library/react";
import { beforeEach, describe, expect, it, vi } from "vitest";
import type {
	StoragePolicy,
	StoragePolicyForcedPurgePreview,
	StoragePolicyRecoveryProbe,
} from "@/types/api";
import { StoragePolicyRecoveryDialog } from "./StoragePolicyRecoveryDialog";

const mocks = vi.hoisted(() => ({
	copy: vi.fn().mockResolvedValue(undefined),
	onConfirmationChange: vi.fn(),
	onOpenChange: vi.fn(),
	onPreviewForcedPurge: vi.fn(),
	onReasonChange: vi.fn(),
	onRetryProbe: vi.fn(),
	onStartForcedPurge: vi.fn(),
	onStartRecovery: vi.fn(),
	onTargetPolicyChange: vi.fn(),
}));

vi.mock("react-i18next", () => ({
	useTranslation: () => ({ t: (key: string) => key }),
}));

vi.mock("@/lib/clipboard", () => ({
	writeTextToClipboard: mocks.copy,
}));

vi.mock("@/components/ui/select", () => ({
	Select: ({
		disabled,
		items = [],
		onValueChange,
		value,
	}: {
		disabled?: boolean;
		items?: Array<{ label: string; value: string }>;
		onValueChange?: (value: string) => void;
		value?: string;
	}) => (
		<div>
			<button
				type="button"
				role="combobox"
				aria-expanded="false"
				disabled={disabled}
			>
				{items.find((item) => item.value === value)?.label}
			</button>
			{items.map((item) => (
				<button
					type="button"
					role="option"
					key={item.value}
					onClick={() => onValueChange?.(item.value)}
				>
					{item.label}
				</button>
			))}
		</div>
	),
	SelectContent: () => null,
	SelectItem: () => null,
	SelectTrigger: () => null,
	SelectValue: () => null,
}));

function policy(id: number, name: string): StoragePolicy {
	return {
		id,
		name,
		connector_id: "asterdrive.storage.local",
		connector_config: {
			format_version: 1,
			connector_id: "asterdrive.storage.local",
			schema_version: 1,
			values: {},
		},
		behavior: {},
		max_file_size: 0,
		allowed_types: [],
		is_default: false,
		chunk_size: 1024,
		created_at: "2026-09-07T00:00:00Z",
		updated_at: "2026-09-07T00:00:00Z",
	};
}

const probe: StoragePolicyRecoveryProbe = {
	policy_id: 1,
	policy_updated_at: "2026-09-07T00:00:00Z",
	status: "recoverable",
	can_start_recovery: true,
	stored_blob_count: 2,
	stored_total_bytes: 128,
	virtual_empty_blob_count: 1,
	samples: [
		{
			blob_id: 10,
			status: "readable",
			retryable: false,
		},
	],
	plan_hash: "probe-hash",
};

const purgePreview: StoragePolicyForcedPurgePreview = {
	policy_id: 1,
	policy_name: "Source",
	policy_updated_at: "2026-09-07T00:00:00Z",
	blob_count: 2,
	blob_total_bytes: 128,
	file_count: 1,
	trash_file_count: 0,
	affected_revision_count: 1,
	affected_logical_bytes: 128,
	direct_share_count: 0,
	placement_target_count: 0,
	upload_session_count: 1,
	can_start: true,
	confirmation_phrase: "DELETE Source",
	impact_digest: "impact-hash",
};

function renderDialog(
	overrides: Partial<
		React.ComponentProps<typeof StoragePolicyRecoveryDialog>
	> = {},
) {
	return render(
		<StoragePolicyRecoveryDialog
			confirmation=""
			loading={false}
			open
			policies={[policy(1, "Source"), policy(2, "Archive target")]}
			policy={policy(1, "Source")}
			probe={probe}
			purgePreview={purgePreview}
			reason=""
			submitting={false}
			targetPolicyId="2"
			onConfirmationChange={mocks.onConfirmationChange}
			onOpenChange={mocks.onOpenChange}
			onPreviewForcedPurge={mocks.onPreviewForcedPurge}
			onReasonChange={mocks.onReasonChange}
			onRetryProbe={mocks.onRetryProbe}
			onStartForcedPurge={mocks.onStartForcedPurge}
			onStartRecovery={mocks.onStartRecovery}
			onTargetPolicyChange={mocks.onTargetPolicyChange}
			{...overrides}
		/>,
	);
}

describe("StoragePolicyRecoveryDialog", () => {
	beforeEach(() => {
		vi.clearAllMocks();
	});

	it("shows the selected target policy label instead of its raw id", () => {
		renderDialog();
		expect(screen.getByRole("combobox")).toHaveTextContent(
			"#2 · Archive target",
		);
	});

	it("reports target policy selection changes", () => {
		renderDialog({
			policies: [
				policy(1, "Source"),
				policy(2, "Archive target"),
				policy(3, "Secondary target"),
			],
		});
		fireEvent.click(
			screen.getByRole("option", { name: "#3 · Secondary target" }),
		);
		expect(mocks.onTargetPolicyChange).toHaveBeenCalledWith("3");
	});

	it("copies and fills the server-generated confirmation phrase", async () => {
		renderDialog();
		fireEvent.click(screen.getByTitle("policy_forced_purge_copy_confirmation"));
		await waitFor(() => {
			expect(mocks.copy).toHaveBeenCalledWith("DELETE Source");
		});
		expect(mocks.onConfirmationChange).toHaveBeenCalledWith("DELETE Source");
	});

	it("keeps the confirmation field usable when clipboard access fails", async () => {
		mocks.copy.mockRejectedValueOnce(new Error("clipboard blocked"));
		renderDialog();
		fireEvent.click(screen.getByTitle("policy_forced_purge_copy_confirmation"));
		await waitFor(() => expect(mocks.copy).toHaveBeenCalled());
		expect(mocks.onConfirmationChange).toHaveBeenCalledWith("DELETE Source");
	});

	it("allows destructive confirmation with an empty optional reason", () => {
		renderDialog({ confirmation: "DELETE Source", reason: "" });
		expect(
			screen.getByRole("button", { name: "policy_forced_purge_start" }),
		).toBeEnabled();
	});

	it("starts recovery when the probe and target are valid", () => {
		renderDialog();
		fireEvent.click(
			screen.getByRole("button", { name: "policy_recovery_start" }),
		);
		expect(mocks.onStartRecovery).toHaveBeenCalledOnce();
	});

	it("starts forced purge after the generated phrase is confirmed", () => {
		renderDialog({ confirmation: "DELETE Source" });
		fireEvent.click(
			screen.getByRole("button", { name: "policy_forced_purge_start" }),
		);
		expect(mocks.onStartForcedPurge).toHaveBeenCalledOnce();
	});

	it("requests a forced purge preview before confirmation controls exist", () => {
		renderDialog({ purgePreview: null });
		fireEvent.click(
			screen.getByRole("button", { name: "policy_forced_purge_preview" }),
		);
		expect(mocks.onPreviewForcedPurge).toHaveBeenCalledOnce();
	});

	it("wires diagnostics, editable purge fields, retry, and close actions", () => {
		renderDialog({
			probe: {
				...probe,
				can_start_recovery: false,
				status: "blocked",
				samples: [
					{
						blob_id: 12,
						diagnostic: "source object denied",
						error_kind: "permission",
						retryable: false,
						status: "blocked",
					},
				],
			},
		});

		expect(screen.getByText(/source object denied/)).toBeInTheDocument();
		fireEvent.change(screen.getByLabelText("policy_forced_purge_reason"), {
			target: { value: "endpoint retired" },
		});
		fireEvent.change(
			screen.getByLabelText("policy_forced_purge_confirmation_label"),
			{ target: { value: "DELETE Source" } },
		);
		fireEvent.click(
			screen.getByRole("button", { name: "policy_recovery_probe_retry" }),
		);
		fireEvent.click(screen.getByRole("button", { name: "core:cancel" }));

		expect(mocks.onReasonChange).toHaveBeenCalledWith("endpoint retired");
		expect(mocks.onConfirmationChange).toHaveBeenCalledWith("DELETE Source");
		expect(mocks.onRetryProbe).toHaveBeenCalledOnce();
		expect(mocks.onOpenChange).toHaveBeenCalledWith(false);
	});

	it("keeps destructive confirmation disabled while placement blockers remain", () => {
		renderDialog({
			confirmation: "DELETE Source",
			purgePreview: {
				...purgePreview,
				can_start: false,
				placement_target_count: 1,
			},
		});
		expect(
			screen.getByRole("button", { name: "policy_forced_purge_start" }),
		).toBeDisabled();
	});
});
