import { describe, expect, it } from "vitest";
import type {
	StorageConnectorDescriptor,
	StorageConnectorFieldDescriptor,
	StoragePolicy,
} from "@/types/api";
import { emptyForm, getPolicyForm } from "./formTypes";
import { policyFormHasUnsavedChanges } from "./policyFormComparison";

function field(
	name: string,
	scope: StorageConnectorFieldDescriptor["scope"] = "connector_config",
	overrides: Partial<StorageConnectorFieldDescriptor> = {},
): StorageConnectorFieldDescriptor {
	return {
		kind: "text",
		label_key: name,
		name,
		required: false,
		scope,
		secret: false,
		...overrides,
	};
}

function descriptor(fields: StorageConnectorFieldDescriptor[]) {
	return {
		fields,
		capabilities: { object_storage_transfer_strategy: false },
	} as StorageConnectorDescriptor;
}

function policy(overrides: Partial<StoragePolicy> = {}): StoragePolicy {
	return {
		allowed_types: [],
		behavior: {
			media_metadata_extensions: [],
			thumbnail_extensions: [],
			thumbnail_processor: null,
		},
		chunk_size: 5 * 1024 * 1024,
		connector_config: {
			connector_id: "plugin.storage",
			format_version: 1,
			schema_version: 1,
			values: {},
		},
		connector_id: "plugin.storage",
		created_at: "2026-01-01T00:00:00Z",
		id: 1,
		is_default: false,
		max_file_size: 0,
		name: "Storage",
		updated_at: "2026-01-01T00:00:00Z",
		...overrides,
	};
}

describe("policyFormComparison", () => {
	it("does not report changes without an editing policy", () => {
		expect(policyFormHasUnsavedChanges(emptyForm, null)).toBe(false);
	});

	it("compares the connector-owned config envelope and trims declared fields", () => {
		const saved = policy({
			connector_config: {
				connector_id: "plugin.storage",
				format_version: 1,
				schema_version: 1,
				values: { endpoint: "https://storage.example.test", mode: "direct" },
			},
		});
		const schema = descriptor([
			field("endpoint", "connector_config", { trim_on_blur: true }),
			field("mode"),
		]);
		const form = {
			...getPolicyForm(saved),
			connector_config_values: {
				endpoint: "  https://storage.example.test  ",
				mode: "direct",
			},
		};

		expect(policyFormHasUnsavedChanges(form, saved, schema)).toBe(false);
		expect(
			policyFormHasUnsavedChanges(
				{
					...form,
					connector_config_values: {
						...form.connector_config_values,
						mode: "relay",
					},
				},
				saved,
				schema,
			),
		).toBe(true);
	});

	it("ignores empty credential inputs and detects non-empty replacements", () => {
		const saved = policy();
		const schema = descriptor([
			field("access_key", "static_credential", { trim_on_blur: true }),
			field("secret_key", "static_credential", {
				kind: "secret",
				secret: true,
			}),
		]);
		const form = {
			...getPolicyForm(saved),
			credential_values: { access_key: "  ", secret_key: "" },
		};

		expect(policyFormHasUnsavedChanges(form, saved, schema)).toBe(false);
		expect(
			policyFormHasUnsavedChanges(
				{ ...form, credential_values: { access_key: "new-key" } },
				saved,
				schema,
			),
		).toBe(true);
	});

	it("detects policy behavior arrays, limits, default state, and connector changes", () => {
		const saved = policy({
			behavior: {
				media_metadata_extensions: ["mp4"],
				thumbnail_extensions: ["jpg"],
				thumbnail_processor: "storage_native",
			},
			is_default: true,
			max_file_size: 1024,
		});
		const form = getPolicyForm(saved);

		expect(policyFormHasUnsavedChanges(form, saved)).toBe(false);
		expect(
			policyFormHasUnsavedChanges(
				{ ...form, thumbnail_extensions: ["png"] },
				saved,
			),
		).toBe(true);
		expect(
			policyFormHasUnsavedChanges(
				{ ...form, connector_id: "plugin.other" },
				saved,
			),
		).toBe(true);
	});

	it("treats malformed connector envelopes as empty without throwing", () => {
		const saved = policy({ connector_config: "broken" as never });
		const form = getPolicyForm(saved);

		expect(form.connector_config_values).toEqual({});
		expect(form.connector_id).toBe("plugin.storage");
		expect(policyFormHasUnsavedChanges(form, saved)).toBe(false);
	});
});
