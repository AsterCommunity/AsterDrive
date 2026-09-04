import { describe, expect, it } from "vitest";
import { getRemoteNodeRemoteStorageTargetProfileStatus } from "@/components/admin/admin-remote-nodes-page/remoteNodeRemoteStorageTargetPresentation";
import type { RemoteStorageTargetInfo } from "@/types/api";

const profile = (
	overrides: Partial<RemoteStorageTargetInfo> = {},
): RemoteStorageTargetInfo => ({
	applied_revision: 3,
	connector_config: {
		format_version: 1,
		connector_id: "asterdrive.storage.local",
		schema_version: 1,
		values: { base_path: "incoming" },
	},
	connector_id: "asterdrive.storage.local",
	created_at: "2026-05-01T00:00:00Z",
	desired_revision: 3,
	is_default: false,
	last_error: "",
	name: "Default",
	target_key: "default",
	updated_at: "2026-05-02T00:00:00Z",
	...overrides,
});

describe("remoteNodeRemoteStorageTargetPresentation", () => {
	it("prioritizes error status over revision drift", () => {
		expect(
			getRemoteNodeRemoteStorageTargetProfileStatus(
				profile({
					applied_revision: 1,
					desired_revision: 3,
					last_error: "sync failed",
				}),
			),
		).toMatchObject({
			labelKey: "remote_node_ingress_profile_status_error",
			toneClass: expect.stringContaining("destructive"),
		});
	});

	it("detects pending and ready profile statuses", () => {
		expect(
			getRemoteNodeRemoteStorageTargetProfileStatus(
				profile({ applied_revision: 1, desired_revision: 2 }),
			),
		).toMatchObject({
			labelKey: "remote_node_ingress_profile_status_pending",
			toneClass: expect.stringContaining("amber"),
		});
		expect(
			getRemoteNodeRemoteStorageTargetProfileStatus(profile()),
		).toMatchObject({
			labelKey: "remote_node_ingress_profile_status_ready",
			toneClass: expect.stringContaining("emerald"),
		});
	});
});
