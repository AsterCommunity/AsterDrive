import { describe, expect, it } from "vitest";
import type {
	StorageConnectorDescriptor,
	StorageConnectorFieldDescriptor,
} from "@/types/api";
import {
	getEndpointValidationMessage,
	normalizeConnectorFieldValue,
	normalizePolicyForm,
} from "./connectionNormalization";
import { emptyForm } from "./formTypes";

function field(
	name: string,
	overrides: Partial<StorageConnectorFieldDescriptor> = {},
): StorageConnectorFieldDescriptor {
	return {
		kind: "text",
		label_key: name,
		name,
		required: false,
		scope: "connector_config",
		secret: false,
		...overrides,
	};
}

function descriptor(
	fields: StorageConnectorFieldDescriptor[],
): StorageConnectorDescriptor {
	return { fields } as StorageConnectorDescriptor;
}

describe("connector field normalization", () => {
	it("uses a connector-neutral endpoint protocol fallback for stale descriptors", () => {
		const schema = descriptor([
			field("endpoint", {
				allowed_endpoint_protocols: ["https:"],
				required: true,
			}),
		]);
		const form = {
			...emptyForm,
			connector_config_values: { endpoint: "http://archive.example.test" },
		};

		expect(getEndpointValidationMessage(form, (key) => key, schema)).toBe(
			"policy_connector_endpoint_protocol_invalid",
		);
	});

	it("applies descriptor defaults to missing fields in both default modes", () => {
		const schema = descriptor([
			field("region", { default_value: "auto" }),
			field("base_path", {
				default_mode: "missing_or_empty_text",
				default_value: "./data/uploads",
			}),
		]);

		expect(
			normalizePolicyForm(emptyForm, schema).connector_config_values,
		).toEqual({
			base_path: "./data/uploads",
			region: "auto",
		});
	});

	it("distinguishes ordinary optional empty text from empty-text defaults", () => {
		const ordinary = field("prefix", { default_value: "objects" });
		const localPath = field("base_path", {
			default_mode: "missing_or_empty_text",
			default_value: "./data/uploads",
		});

		expect(normalizeConnectorFieldValue(ordinary, "")).toBe("");
		expect(normalizeConnectorFieldValue(localPath, "")).toBe("./data/uploads");
	});

	it("trims supplied text before resolving an empty-text default", () => {
		const localPath = field("base_path", {
			default_mode: "missing_or_empty_text",
			default_value: "./data/uploads",
			trim_on_blur: true,
		});

		expect(normalizeConnectorFieldValue(localPath, "   ")).toBe(
			"./data/uploads",
		);
	});

	it("keeps explicit scalar zero, false, and null values", () => {
		const numberField = field("retries", {
			default_value: 3,
			kind: "number",
		});
		const booleanField = field("enabled", {
			default_value: true,
			kind: "boolean",
		});

		expect(normalizeConnectorFieldValue(numberField, 0)).toBe(0);
		expect(normalizeConnectorFieldValue(booleanField, false)).toBe(false);
		expect(normalizeConnectorFieldValue(field("optional"), null)).toBeNull();
	});

	it("returns a new normalized map without mutating the form", () => {
		const form = {
			...emptyForm,
			connector_config_values: { base_path: "" },
		};
		const schema = descriptor([
			field("base_path", {
				default_mode: "missing_or_empty_text",
				default_value: "./data/uploads",
			}),
		]);

		const normalized = normalizePolicyForm(form, schema);
		expect(normalized.connector_config_values.base_path).toBe("./data/uploads");
		expect(form.connector_config_values.base_path).toBe("");
	});
});
