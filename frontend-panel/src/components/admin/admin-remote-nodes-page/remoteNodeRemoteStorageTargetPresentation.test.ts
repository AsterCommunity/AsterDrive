import { describe, expect, it } from "vitest";
import type { RemoteStorageTargetInfo } from "@/types/api";
import {
	getRemoteNodeRemoteStorageTargetConnectorBadgeTone,
	getRemoteNodeRemoteStorageTargetProfileStatus,
} from "./remoteNodeRemoteStorageTargetPresentation";

const target = (
	overrides: Partial<RemoteStorageTargetInfo> = {},
): RemoteStorageTargetInfo => ({
	target_key: "target",
	name: "Target",
	connector_id: "plugin.example.archive",
	connector_config: {
		format_version: 1,
		connector_id: "plugin.example.archive",
		schema_version: 1,
		values: {},
	},
	credential_configured: false,
	connector_available: true,
	is_default: false,
	desired_revision: 3,
	applied_revision: 3,
	last_error: "",
	created_at: "",
	updated_at: "",
	...overrides,
});
describe("remote target presentation", () => {
	it("prioritizes unavailable, error, pending, and ready states", () => {
		expect(
			getRemoteNodeRemoteStorageTargetProfileStatus(
				target({ connector_available: false }),
			).labelKey,
		).toContain("unavailable");
		expect(
			getRemoteNodeRemoteStorageTargetProfileStatus(
				target({ last_error: "failed" }),
			).labelKey,
		).toContain("error");
		expect(
			getRemoteNodeRemoteStorageTargetProfileStatus(
				target({ applied_revision: 1 }),
			).labelKey,
		).toContain("pending");
		expect(
			getRemoteNodeRemoteStorageTargetProfileStatus(target()).labelKey,
		).toContain("ready");
	});
	it("uses availability rather than connector identity for badge tone", () => {
		expect(getRemoteNodeRemoteStorageTargetConnectorBadgeTone(true)).toContain(
			"blue",
		);
		expect(getRemoteNodeRemoteStorageTargetConnectorBadgeTone(false)).toContain(
			"slate",
		);
	});
});
