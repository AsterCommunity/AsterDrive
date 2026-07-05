import { describe, expect, it } from "vitest";
import type {
	DriverType,
	StorageConnectorDescriptor,
	StoragePolicy,
} from "@/types/api";
import { getPolicyConnectionTestKey } from "./connectionNormalization";
import { emptyForm } from "./formTypes";
import {
	selectStoragePolicyActionValueSource,
	selectStoragePolicyConnectionTestMode,
	shouldRunPolicyConnectionSaveTest,
} from "./policyActionSelection";

function descriptor(
	driverType: DriverType,
	actions: StorageConnectorDescriptor["actions"],
): StorageConnectorDescriptor {
	return {
		actions,
		authorization_provider: null,
		capabilities: {
			capacity: true,
			efficient_range: true,
			list: true,
			object_storage_transfer_strategy: false,
			presigned_download: false,
			remote_node_binding: false,
			storage_native_media_metadata: false,
			storage_native_thumbnail: false,
		},
		credential_mode: "none",
		description: `${driverType} descriptor`,
		driver_recommendations: [],
		driver_type: driverType,
		enabled: true,
		fields: [
			{
				kind: "text",
				name: "base_path",
				required: false,
				scope: "connection",
				secret: false,
			},
		],
		label: driverType,
		related_issues: [],
		requires_authorization: false,
		ui: {
			description_key: `${driverType}_description`,
			icon: null,
			label_key: driverType,
		},
		upload_workflows: {
			frontend_direct_provider_resumable_upload: false,
			object_multipart_upload: false,
			presigned_upload: false,
			provider_resumable_upload: false,
			simple_upload: true,
			stream_upload: true,
		},
	};
}

const draftAction = {
	affordance_action: "test_draft_connection",
	endpoints: ["test_policy_params"],
	kind: "connection_test",
	mutates_remote_state: false,
	requires_authorization: false,
	requires_saved_policy: false,
} as const;

const savedAction = {
	affordance_action: "test_saved_connection",
	endpoints: ["test_policy_connection"],
	kind: "connection_test",
	mutates_remote_state: false,
	requires_authorization: false,
	requires_saved_policy: true,
} as const;

function policy(overrides: Partial<StoragePolicy> = {}): StoragePolicy {
	return {
		base_path: "",
		bucket: "",
		created_at: "2026-01-01T00:00:00Z",
		driver_type: "local",
		endpoint: "",
		id: 7,
		is_default: false,
		max_file_size: null,
		name: "Local",
		options: {},
		remote_node_id: null,
		remote_storage_target_key: null,
		updated_at: "2026-01-01T00:00:00Z",
		...overrides,
	};
}

describe("policy action selection", () => {
	it("uses draft values for new policies and changed edits", () => {
		const localDescriptor = descriptor("local", [draftAction, savedAction]);

		expect(
			selectStoragePolicyActionValueSource({
				descriptor: localDescriptor,
				editingId: null,
				editingPolicy: null,
				form: emptyForm,
			}),
		).toBe("draft");

		expect(
			selectStoragePolicyActionValueSource({
				descriptor: localDescriptor,
				editingId: 7,
				editingPolicy: policy(),
				form: { ...emptyForm, base_path: "/data" },
			}),
		).toBe("draft");
	});

	it("uses saved values for unchanged edits", () => {
		const localDescriptor = descriptor("local", [draftAction, savedAction]);

		expect(
			selectStoragePolicyActionValueSource({
				descriptor: localDescriptor,
				editingId: 7,
				editingPolicy: policy(),
				form: emptyForm,
			}),
		).toBe("saved");
	});

	it("returns unsupported when the descriptor lacks the selected connection test mode", () => {
		const savedOnlyDescriptor = descriptor("local", [savedAction]);
		const draftOnlyDescriptor = descriptor("local", [draftAction]);

		expect(
			selectStoragePolicyConnectionTestMode({
				descriptor: savedOnlyDescriptor,
				editingId: null,
				editingPolicy: null,
				form: emptyForm,
			}),
		).toBe("unsupported");

		expect(
			selectStoragePolicyConnectionTestMode({
				descriptor: draftOnlyDescriptor,
				editingId: 7,
				editingPolicy: policy(),
				form: emptyForm,
			}),
		).toBe("unsupported");
	});

	it("skips save-time connection tests after the current connection key was validated", () => {
		const localDescriptor = descriptor("local", [draftAction]);
		const form = { ...emptyForm, base_path: "/data" };
		const validatedConnectionKey = getPolicyConnectionTestKey(
			form,
			localDescriptor,
		);

		expect(
			shouldRunPolicyConnectionSaveTest({
				descriptor: localDescriptor,
				editingId: null,
				editingPolicy: null,
				form,
				validatedConnectionKey,
			}),
		).toBe(false);
		expect(
			shouldRunPolicyConnectionSaveTest({
				descriptor: localDescriptor,
				editingId: null,
				editingPolicy: null,
				form,
				validatedConnectionKey: null,
			}),
		).toBe(true);
	});
});
