import { describe, expect, it } from "vitest";
import type { StorageConnectorDescriptor } from "@/types/api";
import { emptyForm } from "./formTypes";
import {
	applyStorageConnectorPromotion,
	findStorageConnectorPromotionCandidates,
} from "./policyPromotion";

const source = {
	connector_id: "asterdrive.storage.s3",
} as StorageConnectorDescriptor;

const target = {
	connector_id: "asterdrive.storage.tencent_cos",
	capabilities: {
		storage_native_thumbnail: true,
		storage_native_media_metadata: true,
	},
	fields: [
		{
			name: "base_path",
			scope: "connector_config",
			kind: "text",
			default_value: "",
		},
	],
	promotions: [
		{
			promotion_id: "promote_from_s3",
			source_connector_id: source.connector_id,
			description_key: "promotion_desc",
			confirmation_key: "promotion_confirm",
			requirements: [
				{
					source_field: "endpoint",
					matcher: {
						kind: "url_host_suffix",
						suffix: ".myqcloud.com",
					},
				},
			],
			config_mappings: [
				{
					source_field: "endpoint",
					target_field: "endpoint",
				},
				{
					source_field: "bucket",
					target_field: "bucket",
					preserve_value: true,
				},
				{
					source_field: "base_path",
					target_field: "base_path",
					preserve_value: true,
				},
			],
			credential_mappings: [
				{
					source_field: "s3_access_key_id",
					target_field: "tencent_cos_secret_id",
				},
				{
					source_field: "s3_secret_access_key",
					target_field: "tencent_cos_secret_key",
				},
			],
		},
	],
} as StorageConnectorDescriptor;

function s3Form(endpoint: string) {
	return {
		...emptyForm,
		name: "Existing policy",
		connector_id: source.connector_id,
		connector_config_values: {
			endpoint,
			bucket: "media-1250000000",
			base_path: "tenant/files",
			s3_path_style: true,
		},
		credential_values: {
			s3_access_key_id: "AKIDEXAMPLE",
			s3_secret_access_key: "SECRETEXAMPLE",
		},
	};
}

describe("storage connector promotion", () => {
	it("discovers target-owned promotions from matching source values", () => {
		expect(
			findStorageConnectorPromotionCandidates(
				[source, target],
				s3Form("https://media-1250000000.cos.ap-guangzhou.myqcloud.com"),
			),
		).toHaveLength(1);
		expect(
			findStorageConnectorPromotionCandidates(
				[source, target],
				s3Form("https://s3.example.test"),
			),
		).toHaveLength(0);
	});

	it("maps compatible draft config and credentials without provider branches", () => {
		const form = s3Form(
			"https://media-1250000000.cos.ap-guangzhou.myqcloud.com",
		);
		const [candidate] = findStorageConnectorPromotionCandidates(
			[source, target],
			form,
		);
		const promoted = applyStorageConnectorPromotion(form, candidate);

		expect(promoted.name).toBe("Existing policy");
		expect(promoted.connector_id).toBe(target.connector_id);
		expect(promoted.connector_config_values).toMatchObject({
			endpoint: form.connector_config_values.endpoint,
			bucket: "media-1250000000",
			base_path: "tenant/files",
		});
		expect(promoted.connector_config_values).not.toHaveProperty(
			"s3_path_style",
		);
		expect(promoted.credential_values).toEqual({
			tencent_cos_secret_id: "AKIDEXAMPLE",
			tencent_cos_secret_key: "SECRETEXAMPLE",
		});
	});

	it("evaluates portable string matchers and requires every condition", () => {
		const matcherTarget = structuredClone(target);
		matcherTarget.promotions = [
			{
				...target.promotions?.[0],
				config_mappings: target.promotions?.[0]?.config_mappings ?? [],
				confirmation_key: "promotion_confirm",
				description_key: "promotion_desc",
				promotion_id: "match_strings",
				requirements: [
					{
						source_field: "endpoint",
						matcher: {
							kind: "string_prefix",
							prefix: "https://",
						},
					},
					{
						source_field: "provider",
						matcher: {
							kind: "string_equals",
							value: "tencent-cos",
						},
					},
					{
						source_field: "bucket",
						matcher: {
							kind: "string_suffix",
							suffix: "-prod",
						},
					},
					{
						source_field: "region",
						matcher: {
							kind: "string_equals",
							value: "auto",
						},
						negate: true,
					},
					{
						source_field: "endpoint",
						matcher: {
							kind: "url_host_suffix",
							suffix: "-internal.example.test",
						},
						negate: true,
					},
				],
				source_connector_id: source.connector_id,
			},
		];
		const form = s3Form("https://s3.example.test");
		form.connector_config_values.provider = "Tencent-COS";
		form.connector_config_values.bucket = "archive-PROD";
		form.connector_config_values.region = "cn-east-1";
		expect(
			findStorageConnectorPromotionCandidates([matcherTarget], form),
		).toHaveLength(1);

		const caseSensitive = structuredClone(matcherTarget);
		const providerRequirement =
			caseSensitive.promotions?.[0]?.requirements?.find(
				(requirement) =>
					requirement.source_field === "provider" &&
					requirement.matcher.kind === "string_equals",
			);
		if (providerRequirement?.matcher.kind === "string_equals") {
			providerRequirement.matcher.case_sensitive = true;
		}
		expect(
			findStorageConnectorPromotionCandidates([caseSensitive], form),
		).toHaveLength(0);

		form.connector_config_values.bucket = "archive-dev";
		expect(
			findStorageConnectorPromotionCandidates([matcherTarget], form),
		).toHaveLength(0);

		form.connector_config_values.bucket = "archive-prod";
		form.connector_config_values.region = "AUTO";
		expect(
			findStorageConnectorPromotionCandidates([matcherTarget], form),
		).toHaveLength(0);

		form.connector_config_values.region = "cn-east-1";
		form.connector_config_values.endpoint = "https://s3-internal.example.test";
		expect(
			findStorageConnectorPromotionCandidates([matcherTarget], form),
		).toHaveLength(0);
	});

	it("rejects invalid and lookalike URL hosts", () => {
		for (const endpoint of [
			"not a url",
			"https://evilmyqcloud.com",
			"https://myqcloud.com",
		]) {
			expect(
				findStorageConnectorPromotionCandidates([target], s3Form(endpoint)),
			).toHaveLength(0);
		}
	});

	it("keeps target defaults for unmapped values and preserves supported behavior", () => {
		const form = s3Form(
			"https://media-1250000000.cos.ap-guangzhou.myqcloud.com",
		);
		form.storage_native_thumbnail_enabled = true;
		form.storage_native_media_metadata_enabled = true;
		delete form.connector_config_values.base_path;
		const [candidate] = findStorageConnectorPromotionCandidates([target], form);
		const promoted = applyStorageConnectorPromotion(form, candidate);

		expect(promoted.connector_config_values.base_path).toBe("");
		expect(promoted.storage_native_thumbnail_enabled).toBe(true);
		expect(promoted.storage_native_media_metadata_enabled).toBe(true);
	});

	it("returns each matching target in descriptor order", () => {
		const secondTarget = structuredClone(target);
		secondTarget.connector_id = "com.example.second";
		const secondPromotion = secondTarget.promotions?.[0];
		if (!secondPromotion) {
			throw new Error("promotion fixture is missing");
		}
		secondPromotion.promotion_id = "second_promotion";
		const candidates = findStorageConnectorPromotionCandidates(
			[target, secondTarget],
			s3Form("https://media-1250000000.cos.ap-guangzhou.myqcloud.com"),
		);
		expect(
			candidates.map((item) => item.targetDescriptor.connector_id),
		).toEqual([target.connector_id, secondTarget.connector_id]);
	});

	it("handles omitted requirements, explicit fields, and credential mappings", () => {
		const targetWithoutOptionalPromotionFields = structuredClone(target);
		const promotion = targetWithoutOptionalPromotionFields.promotions?.[0];
		if (!promotion) {
			throw new Error("promotion fixture is missing");
		}
		delete promotion.requirements;
		delete promotion.credential_mappings;

		const form = s3Form("https://unmatched.example.test");
		form.connector_config_explicit_fields = undefined;
		const [candidate] = findStorageConnectorPromotionCandidates(
			[targetWithoutOptionalPromotionFields],
			form,
		);
		const promoted = applyStorageConnectorPromotion(form, candidate);
		expect(promoted.connector_config_explicit_fields).toEqual([
			"endpoint",
			"bucket",
			"base_path",
		]);
		expect(promoted.credential_values).toEqual({});

		const missingCredentialForm = s3Form(
			"https://media-1250000000.cos.ap-guangzhou.myqcloud.com",
		);
		delete missingCredentialForm.credential_values.s3_secret_access_key;
		const [credentialCandidate] = findStorageConnectorPromotionCandidates(
			[target],
			missingCredentialForm,
		);
		const partiallyMapped = applyStorageConnectorPromotion(
			missingCredentialForm,
			credentialCandidate,
		);
		expect(partiallyMapped.credential_values).toEqual({
			tencent_cos_secret_id: "AKIDEXAMPLE",
		});
	});
});
