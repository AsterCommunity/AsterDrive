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
	onStartForcedPurge: vi.fn(),
}));

vi.mock("react-i18next", () => ({
	useTranslation: () => ({ t: (key: string) => key }),
}));

vi.mock("@/lib/clipboard", () => ({
	writeTextToClipboard: mocks.copy,
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
			onOpenChange={vi.fn()}
			onPreviewForcedPurge={vi.fn()}
			onReasonChange={vi.fn()}
			onRetryProbe={vi.fn()}
			onStartForcedPurge={mocks.onStartForcedPurge}
			onStartRecovery={vi.fn()}
			onTargetPolicyChange={vi.fn()}
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

	it("copies and fills the server-generated confirmation phrase", async () => {
		renderDialog();
		fireEvent.click(screen.getByTitle("policy_forced_purge_copy_confirmation"));
		await waitFor(() => {
			expect(mocks.copy).toHaveBeenCalledWith("DELETE Source");
		});
		expect(mocks.onConfirmationChange).toHaveBeenCalledWith("DELETE Source");
	});

	it("allows destructive confirmation with an empty optional reason", () => {
		renderDialog({ confirmation: "DELETE Source", reason: "" });
		expect(
			screen.getByRole("button", { name: "policy_forced_purge_start" }),
		).toBeEnabled();
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
