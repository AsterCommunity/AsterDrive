import { describe, expect, it } from "vitest";
import {
	buildPolicyGroupPayload,
	buildPolicyGroupRuleForm,
	bytesToMbInput,
	getDefaultPolicyGroupForm,
	validatePolicyGroupForm,
} from "@/components/admin/policyGroupEditorShared";
import type { StoragePolicy } from "@/types/api";

const t = (key: string) => key;

describe("policyGroupEditorShared", () => {
	it("creates a default form seeded from the first policy", () => {
		const form = getDefaultPolicyGroupForm([
			{ id: 8, name: "Primary" } as StoragePolicy,
		]);

		expect(form.items).toHaveLength(1);
		expect(form.items[0]?.targets).toHaveLength(1);
		expect(form.items[0]?.targets[0]?.policyId).toBe("8");
		expect(form.items[0]?.targets[0]?.weight).toBe("100");
		expect(form.items[0]?.name).toBe("Rule 1");
	});

	it("preserves custom rule names and sends edited names", () => {
		const rule = buildPolicyGroupRuleForm(1, 0, 0, {
			name: "Images to primary",
			selection_mode: "first_available",
			unavailable_behavior: "next_rule",
			targets: [],
		});

		expect(rule.name).toBe("Images to primary");
		const payload = buildPolicyGroupPayload({
			name: "Named",
			description: "",
			isEnabled: true,
			isDefault: false,
			items: [rule],
		});
		expect(payload.rules?.[0]?.name).toBe("Images to primary");
	});

	it("validates blank and oversized rule names", () => {
		const rule = {
			name: " ",
			key: "a",
			minFileSizeMb: "",
			maxFileSizeMb: "",
			selectionMode: "first_available" as const,
			unavailableBehavior: "next_rule" as const,
			targets: [
				{
					key: "a-1",
					policyId: "1",
					weight: "100",
					isEnabled: true,
					acceptingNewWrites: true,
				},
			],
		};
		const form = {
			name: "Named",
			description: "",
			isEnabled: true,
			isDefault: false,
			items: [rule],
		};
		expect(validatePolicyGroupForm(form, 1, t)).toBe(
			"policy_group_rule_name_required",
		);
		expect(
			validatePolicyGroupForm(
				{ ...form, items: [{ ...rule, name: "x".repeat(65) }] },
				1,
				t,
			),
		).toBe("policy_group_rule_name_too_long");
	});

	it("validates duplicate primary policies across rules", () => {
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
							name: "Primary",
							minFileSizeMb: "",
							maxFileSizeMb: "",
							selectionMode: "first_available",
							unavailableBehavior: "next_rule",
							targets: [
								{
									key: "a-1",
									policyId: "1",
									weight: "100",
									isEnabled: true,
									acceptingNewWrites: true,
								},
							],
						},
						{
							key: "b",
							name: "Fallback",
							minFileSizeMb: "",
							maxFileSizeMb: "",
							selectionMode: "first_available",
							unavailableBehavior: "next_rule",
							targets: [
								{
									key: "b-1",
									policyId: "1",
									weight: "100",
									isEnabled: true,
									acceptingNewWrites: true,
								},
							],
						},
					],
				},
				2,
				t,
			),
		).toBe("policy_group_rule_policy_duplicate");
	});

	it("rejects invalid policy ids, empty targets and in-rule duplicates", () => {
		const baseRule = {
			name: "Rule",
			minFileSizeMb: "",
			maxFileSizeMb: "",
			selectionMode: "first_available" as const,
			unavailableBehavior: "next_rule" as const,
		};

		expect(
			validatePolicyGroupForm(
				{
					name: "Invalid policy",
					description: "",
					isEnabled: true,
					isDefault: false,
					items: [
						{
							...baseRule,
							key: "a",
							targets: [
								{
									key: "a-1",
									policyId: "abc",
									weight: "100",
									isEnabled: true,
									acceptingNewWrites: true,
								},
							],
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
					name: "No targets",
					description: "",
					isEnabled: true,
					isDefault: false,
					items: [{ ...baseRule, key: "a", targets: [] }],
				},
				1,
				t,
			),
		).toBe("policy_group_rule_target_required");

		expect(
			validatePolicyGroupForm(
				{
					name: "In-rule duplicate",
					description: "",
					isEnabled: true,
					isDefault: false,
					items: [
						{
							...baseRule,
							key: "a",
							targets: [
								{
									key: "a-1",
									policyId: "1",
									weight: "100",
									isEnabled: true,
									acceptingNewWrites: true,
								},
								{
									key: "a-2",
									policyId: "01",
									weight: "50",
									isEnabled: true,
									acceptingNewWrites: true,
								},
							],
						},
					],
				},
				2,
				t,
			),
		).toBe("policy_group_rule_target_duplicate");
	});

	it("rejects invalid target weights", () => {
		expect(
			validatePolicyGroupForm(
				{
					name: "Bad weight",
					description: "",
					isEnabled: true,
					isDefault: false,
					items: [
						{
							key: "a",
							name: "Bad weight",
							minFileSizeMb: "",
							maxFileSizeMb: "",
							selectionMode: "first_available",
							unavailableBehavior: "next_rule",
							targets: [
								{
									key: "a-1",
									policyId: "1",
									weight: "0",
									isEnabled: true,
									acceptingNewWrites: true,
								},
							],
						},
					],
				},
				1,
				t,
			),
		).toBe("policy_group_target_weight_invalid");
	});

	it("derives priority and stable_order from row order and converts MB to bytes", () => {
		expect(
			buildPolicyGroupPayload({
				name: "Tiered",
				description: "Routing rules",
				isEnabled: true,
				isDefault: false,
				items: [
					{
						key: "a",
						name: "Small files",
						minFileSizeMb: "",
						maxFileSizeMb: "10",
						selectionMode: "first_available",
						unavailableBehavior: "next_rule",
						targets: [
							{
								key: "a-1",
								policyId: "1",
								weight: "100",
								isEnabled: true,
								acceptingNewWrites: true,
							},
						],
					},
					{
						key: "b",
						name: "Large files",
						minFileSizeMb: "10",
						maxFileSizeMb: "",
						selectionMode: "first_available",
						unavailableBehavior: "next_rule",
						targets: [
							{
								key: "b-1",
								policyId: "2",
								weight: "70",
								isEnabled: true,
								acceptingNewWrites: true,
							},
							{
								key: "b-2",
								policyId: "3",
								weight: "30",
								isEnabled: true,
								acceptingNewWrites: false,
							},
						],
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
					name: "Small files",
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
					name: "Large files",
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
							weight: 70,
							is_enabled: true,
							accepting_new_writes: true,
							stable_order: 1,
						},
						{
							policy_id: 3,
							weight: 30,
							is_enabled: true,
							accepting_new_writes: false,
							stable_order: 2,
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
						name: "Exact bytes",
						minFileSizeMb: bytesToMbInput(preciseBytes),
						maxFileSizeMb: "",
						originalMinFileSizeBytes: preciseBytes,
						selectionMode: "first_available",
						unavailableBehavior: "next_rule",
						targets: [
							{
								key: "a-1",
								policyId: "1",
								weight: "100",
								isEnabled: true,
								acceptingNewWrites: true,
							},
						],
					},
				],
			}).rules[0]?.matcher.min_file_size,
		).toBe(preciseBytes);
	});

	it("preserves admission, selection mode and fallback behavior", () => {
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
					name: "Weighted",
					minFileSizeMb: "",
					maxFileSizeMb: "",
					selectionMode: "weighted_random",
					unavailableBehavior: "reject",
					targets: [
						{
							key: "wr-1",
							policyId: "1",
							weight: "70",
							isEnabled: true,
							acceptingNewWrites: true,
						},
						{
							key: "wr-2",
							policyId: "2",
							weight: "30",
							isEnabled: true,
							acceptingNewWrites: false,
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
