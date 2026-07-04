import { describe, expect, it } from "vitest";
import {
	getRemoteNodeRemoteStorageTargetDriverBadgeTone,
	getRemoteNodeRemoteStorageTargetProfileStatus,
} from "@/components/admin/admin-remote-nodes-page/remoteNodeRemoteStorageTargetPresentation";
import type { RemoteStorageTargetInfo } from "@/types/api";

const profile = (
	overrides: Partial<RemoteStorageTargetInfo> = {},
): RemoteStorageTargetInfo => ({
	applied_revision: 3,
	base_path: "incoming",
	bucket: "",
	created_at: "2026-05-01T00:00:00Z",
	desired_revision: 3,
	driver_type: "local",
	endpoint: "",
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

	it("maps driver types to badge tones", () => {
		expect(getRemoteNodeRemoteStorageTargetDriverBadgeTone("s3")).toContain(
			"blue",
		);
		expect(getRemoteNodeRemoteStorageTargetDriverBadgeTone("local")).toContain(
			"slate",
		);
	});
});
