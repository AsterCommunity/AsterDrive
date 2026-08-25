import { describe, expect, it } from "vitest";
import {
	buildPolicyGroupPayload,
	bytesToMbInput,
	getDefaultPolicyGroupForm,
	validatePolicyGroupForm,
} from "@/components/admin/policyGroupDialogShared";
import type { StoragePolicy } from "@/types/api";

const t = (key: string) => key;

describe("policyGroupDialogShared", () => {
	it("creates a default form seeded from the first policy", () => {
		const form = getDefaultPolicyGroupForm([
			{ id: 8, name: "Primary" } as StoragePolicy,
		]);

		expect(form.items).toHaveLength(1);
		expect(form.items[0]?.policyId).toBe("8");
		expect(form.items[0]?.priority).toBe("1");
	});

	it("validates duplicate policies and priorities", () => {
		expect(
			validatePolicyGroupForm(
				{
					name: "Duplicated",
					description: "",
					isEnabled: true,
					isDefault: false,
					items: [
						{
							key: "a",
							policyId: "1",
							priority: "1",
							minFileSizeMb: "",
							maxFileSizeMb: "",
						},
						{
							key: "b",
							policyId: "1",
							priority: "2",
							minFileSizeMb: "",
							maxFileSizeMb: "",
						},
					],
				},
				1,
				t,
			),
		).toBe("policy_group_rule_policy_duplicate");

		expect(
			validatePolicyGroupForm(
				{
					name: "Duplicated priority",
					description: "",
					isEnabled: true,
					isDefault: false,
					items: [
						{
							key: "a",
							policyId: "1",
							priority: "1",
							minFileSizeMb: "",
							maxFileSizeMb: "",
						},
						{
							key: "b",
							policyId: "2",
							priority: "1",
							minFileSizeMb: "",
							maxFileSizeMb: "",
						},
					],
				},
				2,
				t,
			),
		).toBe("policy_group_rule_priority_duplicate");
	});

	it("rejects invalid and numerically duplicated policy ids", () => {
		expect(
			validatePolicyGroupForm(
				{
					name: "Invalid policy",
					description: "",
					isEnabled: true,
					isDefault: false,
					items: [
						{
							key: "a",
							policyId: "abc",
							priority: "1",
							minFileSizeMb: "",
							maxFileSizeMb: "",
						},
					],
				},
				1,
				t,
			),
		).toBe("policy_group_rule_policy_required");

		expect(
			validatePolicyGroupForm(
				{
					name: "Numeric duplicate",
					description: "",
					isEnabled: true,
					isDefault: false,
					items: [
						{
							key: "a",
							policyId: "1",
							priority: "1",
							minFileSizeMb: "",
							maxFileSizeMb: "",
						},
						{
							key: "b",
							policyId: "01",
							priority: "2",
							minFileSizeMb: "",
							maxFileSizeMb: "",
						},
					],
				},
				2,
				t,
			),
		).toBe("policy_group_rule_policy_duplicate");
	});

	it("builds sorted payloads and converts megabytes to bytes", () => {
		expect(
			buildPolicyGroupPayload({
				name: "Tiered",
				description: "Routing rules",
				isEnabled: true,
				isDefault: false,
				items: [
					{
						key: "b",
						policyId: "2",
						priority: "2",
						minFileSizeMb: "10",
						maxFileSizeMb: "",
					},
					{
						key: "a",
						policyId: "1",
						priority: "1",
						minFileSizeMb: "",
						maxFileSizeMb: "10",
					},
				],
			}),
		).toEqual({
			name: "Tiered",
			description: "Routing rules",
			is_enabled: true,
			is_default: false,
			admission: {
				allowed_extensions: [],
				denied_extensions: [],
				accept_extensionless: true,
				allowed_categories: [],
				denied_categories: [],
				max_file_size: 0,
			},
			execution_preference: "automatic",
			rules: [
				{
					name: "Rule 1",
					description: "",
					priority: 1,
					is_enabled: true,
					matcher: {
						min_file_size: 0,
						max_file_size: 10 * 1024 * 1024,
						extensions: [],
						compound_extensions: [],
						extensionless: null,
						categories: [],
					},
					selection_mode: "first_available",
					unavailable_behavior: "next_rule",
					targets: [
						{
							policy_id: 1,
							weight: 100,
							is_enabled: true,
							accepting_new_writes: true,
							stable_order: 1,
						},
					],
				},
				{
					name: "Rule 2",
					description: "",
					priority: 2,
					is_enabled: true,
					matcher: {
						min_file_size: 10 * 1024 * 1024,
						max_file_size: 0,
						extensions: [],
						compound_extensions: [],
						extensionless: null,
						categories: [],
					},
					selection_mode: "first_available",
					unavailable_behavior: "next_rule",
					targets: [
						{
							policy_id: 2,
							weight: 100,
							is_enabled: true,
							accepting_new_writes: true,
							stable_order: 1,
						},
					],
				},
			],
		});
	});

	it("preserves exact byte thresholds when editing an existing group", () => {
		const preciseBytes = 12_345;

		expect(
			buildPolicyGroupPayload({
				name: "Tiered",
				description: "",
				isEnabled: true,
				isDefault: false,
				items: [
					{
						key: "a",
						policyId: "1",
						priority: "1",
						minFileSizeMb: bytesToMbInput(preciseBytes),
						maxFileSizeMb: "",
						originalMinFileSizeBytes: preciseBytes,
					},
				],
			}).rules[0]?.matcher.min_file_size,
		).toBe(preciseBytes);
	});

	it("preserves admission, selection mode, fallback and multiple targets", () => {
		const payload = buildPolicyGroupPayload({
			name: "Weighted",
			description: "",
			isEnabled: true,
			isDefault: false,
			admission: {
				allowed_extensions: ["jpg"],
				denied_extensions: ["exe"],
				accept_extensionless: false,
				allowed_categories: ["image"],
				denied_categories: [],
				max_file_size: 99,
			},
			executionPreference: "force_server_stream",
			items: [
				{
					key: "weighted-rule",
					policyId: "1",
					priority: "1",
					minFileSizeMb: "",
					maxFileSizeMb: "",
					selectionMode: "weighted_random",
					unavailableBehavior: "reject",
					targets: [
						{
							policyId: "1",
							weight: 70,
							isEnabled: true,
							acceptingNewWrites: true,
							stableOrder: 1,
						},
						{
							policyId: "2",
							weight: 30,
							isEnabled: true,
							acceptingNewWrites: false,
							stableOrder: 2,
						},
					],
				},
			],
		});

		expect(payload.execution_preference).toBe("force_server_stream");
		expect(payload.admission?.accept_extensionless).toBe(false);
		expect(payload.rules?.[0]?.selection_mode).toBe("weighted_random");
		expect(payload.rules?.[0]?.unavailable_behavior).toBe("reject");
		expect(payload.rules?.[0]?.targets).toHaveLength(2);
		expect(payload.rules?.[0]?.targets[1]?.weight).toBe(30);
	});
});
