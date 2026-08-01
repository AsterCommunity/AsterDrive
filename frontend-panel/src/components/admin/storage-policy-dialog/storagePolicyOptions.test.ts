import { describe, expect, it } from "vitest";
import type { StorageConnectorDescriptor, StoragePolicy } from "@/types/api";
import { emptyForm, getPolicyForm } from "./formTypes";
import {
	buildCreatePolicyPayload,
	buildPolicyTestPayload,
	buildUpdatePolicyPayload,
} from "./payloadBuilders";
import { buildPolicyOptions } from "./storagePolicyOptions";

const s3Descriptor = {
	driver_type: "s3",
	fields: [
		{
			kind: "text",
			label_key: "s3_region",
			name: "s3_region",
			required: false,
			scope: "policy_options",
			secret: false,
			trim_on_blur: true,
		},
	],
} as StorageConnectorDescriptor;

describe("buildPolicyOptions", () => {
	it("includes the normalized S3 signing region in draft, create, and update payloads", () => {
		const form = {
			...emptyForm,
			driver_type: "s3" as const,
			policy_option_values: { s3_region: " us-east-1 " },
		};
		const expectedOptions = { s3_region: "us-east-1" };

		expect(buildPolicyOptions(form, s3Descriptor)).toEqual(expectedOptions);
		expect(buildPolicyTestPayload(form, s3Descriptor).options).toEqual(
			expectedOptions,
		);
		expect(buildCreatePolicyPayload(form, s3Descriptor).options).toEqual(
			expectedOptions,
		);
		expect(buildUpdatePolicyPayload(form, s3Descriptor).options).toEqual(
			expectedOptions,
		);
	});

	it("omits a blank S3 signing region from every payload", () => {
		const form = {
			...emptyForm,
			driver_type: "s3" as const,
			policy_option_values: { s3_region: "  " },
		};

		for (const options of [
			buildPolicyOptions(form, s3Descriptor),
			buildPolicyTestPayload(form, s3Descriptor).options,
			buildCreatePolicyPayload(form, s3Descriptor).options,
			buildUpdatePolicyPayload(form, s3Descriptor).options,
		]) {
			expect(options).not.toHaveProperty("s3_region");
		}
	});

	it("does not submit the S3 region when the descriptor does not declare it", () => {
		const form = {
			...emptyForm,
			driver_type: "s3" as const,
			policy_option_values: { s3_region: "us-east-1" },
		};

		expect(buildPolicyOptions(form)).not.toHaveProperty("s3_region");
		expect(
			buildPolicyOptions(form, { fields: [] } as StorageConnectorDescriptor),
		).not.toHaveProperty("s3_region");
	});

	it("rehydrates a saved S3 region into descriptor-backed form values", () => {
		const form = getPolicyForm({
			id: 7,
			name: "Archive",
			driver_type: "s3",
			endpoint: "https://s3.example.com",
			bucket: "archive",
			base_path: "",
			remote_node_id: null,
			remote_storage_target_key: null,
			max_file_size: 0,
			allowed_types: [],
			options: { s3_region: "ap-southeast-1" },
			is_default: false,
			chunk_size: 5 * 1024 * 1024,
			created_at: "",
			updated_at: "",
		} as StoragePolicy);

		expect(form.policy_option_values?.s3_region).toBe("ap-southeast-1");
		expect(buildUpdatePolicyPayload(form, s3Descriptor).options).toEqual({
			s3_region: "ap-southeast-1",
		});
	});
});
