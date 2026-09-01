import { describe, expect, it } from "vitest";
import type { StorageConnectorDescriptor } from "@/types/api";
import {
	descriptorHasField,
	findConnectorFieldByDataSource,
	supportsStorageConnectorCustomAction,
	supportsStorageCredentialLifecycle,
	supportsStorageNativeProcessing,
} from "./descriptorPredicates";

const descriptor = {
	actions: [
		{
			action_id: "plugin.repair",
			kind: "custom",
		},
	],
	authorization_provider: null,
	capabilities: {
		remote_node_binding: true,
		storage_native_media_metadata: false,
		storage_native_thumbnail: true,
	},
	credential_mode: "oauth_delegated",
	fields: [
		{
			name: "remote_node_id",
			select: { data_source: "remote_nodes" },
		},
	],
} as StorageConnectorDescriptor;

describe("storage connector descriptor predicates", () => {
	it("derives fields, lifecycle, native processing, and custom actions from metadata", () => {
		expect(descriptorHasField(descriptor, "remote_node_id")).toBe(true);
		expect(
			findConnectorFieldByDataSource(descriptor, "remote_nodes")?.name,
		).toBe("remote_node_id");
		expect(supportsStorageCredentialLifecycle(descriptor)).toBe(true);
		expect(supportsStorageNativeProcessing(descriptor)).toBe(true);
		expect(supportsStorageNativeProcessing(null)).toBe(false);
		expect(
			supportsStorageConnectorCustomAction(descriptor, "plugin.repair"),
		).toBe(true);
		expect(
			supportsStorageConnectorCustomAction(descriptor, "plugin.missing"),
		).toBe(false);
	});
});
