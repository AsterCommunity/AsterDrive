import { describe, expect, it } from "vitest";
import {
	buildPolicyGroupPayload,
	buildPolicyGroupRuleForm,
	bytesToMbInput,
	getDefaultPolicyGroupForm,
	getPolicyGroupForm,
	mbInputToBytes,
	validatePolicyGroupForm,
} from "@/components/admin/policyGroupEditorShared";
import type { StoragePolicy } from "@/types/api";

const t = (key: string) => key;

describe("policyGroupEditorShared", () => {
	it("covers empty and invalid form boundaries", () => {
		const base = {
			name: "Named",
			description: "",
			isEnabled: true,
			isDefault: false,
			items: [],
			categoryRestrictionEnabled: false,
		};
		expect(validatePolicyGroupForm({ ...base, name: " " }, 1, t)).toBe(
			"policy_group_name_required",
		);
		expect(
			validatePolicyGroupForm(
				{ ...base, isDefault: true, isEnabled: false },
				1,
				t,
			),
		).toBe("policy_group_default_requires_enabled");
		expect(validatePolicyGroupForm(base, 0, t)).toBe(
			"policy_group_no_policies_available",
		);
		expect(validatePolicyGroupForm(base, 1, t)).toBe(
			"policy_group_rule_required",
		);

		const validRule = {
			key: "rule",
			name: "Rule",
			description: "",
			minFileSizeMb: "1",
			maxFileSizeMb: "",
			selectionMode: "first_available" as const,
			unavailableBehavior: "next_rule" as const,
			targets: [
				{
					key: "target",
					policyId: "1",
					weight: "100",
					acceptingNewWrites: true,
				},
			],
		};
		expect(
			validatePolicyGroupForm(
				{ ...base, items: [{ ...validRule, minFileSizeMb: "x" }] },
				1,
				t,
			),
		).toBe("policy_group_rule_size_invalid");
		expect(
			validatePolicyGroupForm(
				{
					...base,
					items: [{ ...validRule, minFileSizeMb: "2", maxFileSizeMb: "2" }],
				},
				1,
				t,
			),
		).toBe("policy_group_rule_range_invalid");
	});

	it("normalizes size inputs and generates defaults", () => {
		expect(bytesToMbInput(0)).toBe("");
		expect(bytesToMbInput(-1)).toBe("");
		expect(mbInputToBytes(" ")).toBe(0);
		expect(mbInputToBytes("not-a-number")).toBe(0);
		expect(mbInputToBytes("2.5")).toBe(2.5 * 1024 * 1024);
		const fallback = buildPolicyGroupRuleForm(
			null,
			0,
			0,
			{
				name: " ",
				description: "",
				is_enabled: true,
				matcher: undefined,
				selection_mode: undefined,
				unavailable_behavior: undefined,
				targets: [],
			} as never,
			"Fallback",
		);
		expect(fallback.name).toBe("Fallback");
		expect(fallback.targets[0]?.policyId).toBe("");
	});
	it("creates a default form seeded from the first policy", () => {
		const form = getDefaultPolicyGroupForm([
			{ id: 8, name: "Primary" } as StoragePolicy,
		]);

		expect(form.items).toHaveLength(1);
		expect(form.items[0]?.targets).toHaveLength(1);
		expect(form.items[0]?.targets[0]?.policyId).toBe("8");
		expect(form.items[0]?.targets[0]?.weight).toBe("100");
		expect(form.items[0]?.name).toBe("Rule 1");
		expect(form.categoryRestrictionEnabled).toBe(false);
		expect(form.admission?.allowed_categories).toEqual([]);
	});

	it("derives category restriction mode from an existing allowlist", () => {
		const unrestricted = getPolicyGroupForm({
			name: "Open",
			description: "",
			is_enabled: true,
			is_default: false,
			admission: { allowed_categories: [] },
			execution_preference: "automatic",
			rules: [],
		} as never);
		const restricted = getPolicyGroupForm({
			name: "Images",
			description: "",
			is_enabled: true,
			is_default: false,
			admission: { allowed_categories: ["image"] },
			execution_preference: "automatic",
			rules: [],
		} as never);

		expect(unrestricted.categoryRestrictionEnabled).toBe(false);
		expect(restricted.categoryRestrictionEnabled).toBe(true);
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
			categoryRestrictionEnabled: false,
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

	it("preserves disabled rules and complex matcher fields when editing", () => {
		const form = getPolicyGroupForm({
			name: "Named",
			description: "",
			is_enabled: true,
			is_default: false,
			admission: {} as never,
			execution_preference: "automatic",
			rules: [
				{
					id: 1,
					name: "Archives",
					description: "Cold path",
					priority: 1,
					is_enabled: false,
					matcher: {
						min_file_size: 1,
						max_file_size: 2,
						extensions: ["zip"],
						compound_extensions: ["tar.gz"],
						extensionless: false,
						categories: ["archive"],
					},
					selection_mode: "first_available",
					unavailable_behavior: "next_rule",
					targets: [],
				},
			],
		} as never);
		const rule = form.items[0];
		expect(rule?.description).toBe("Cold path");
		expect(rule?.isEnabled).toBe(false);
		expect(rule?.extensions).toEqual(["zip"]);
		expect(rule?.compoundExtensions).toEqual(["tar.gz"]);
		expect(rule?.extensionless).toBe(false);
		expect(rule?.categories).toEqual(["archive"]);
		expect(buildPolicyGroupPayload(form).rules?.[0]).toMatchObject({
			name: "Archives",
			description: "Cold path",
			is_enabled: false,
			matcher: expect.objectContaining({ extensions: ["zip"] }),
		});
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
									acceptingNewWrites: true,
								},
								{
									key: "a-2",
									policyId: "01",
									weight: "50",
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
				categoryRestrictionEnabled: false,
				items: [
					{
						key: "a",
						name: "Small files",
						description: "",
						isEnabled: true,
						extensions: [],
						compoundExtensions: [],
						extensionless: null,
						categories: [],
						minFileSizeMb: "",
						maxFileSizeMb: "10",
						selectionMode: "first_available",
						unavailableBehavior: "next_rule",
						targets: [
							{
								key: "a-1",
								policyId: "1",
								weight: "100",
								acceptingNewWrites: true,
							},
						],
					},
					{
						key: "b",
						name: "Large files",
						description: "",
						isEnabled: true,
						extensions: [],
						compoundExtensions: [],
						extensionless: null,
						categories: [],
						minFileSizeMb: "10",
						maxFileSizeMb: "",
						selectionMode: "first_available",
						unavailableBehavior: "next_rule",
						targets: [
							{
								key: "b-1",
								policyId: "2",
								weight: "70",
								acceptingNewWrites: true,
							},
							{
								key: "b-2",
								policyId: "3",
								weight: "30",
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
							accepting_new_writes: true,
							stable_order: 1,
						},
						{
							policy_id: 3,
							weight: 30,
							accepting_new_writes: false,
							stable_order: 2,
						},
					],
				},
			],
		});
	});

	it("requires a non-empty allowlist only when category restriction is enabled", () => {
		const form = {
			...getDefaultPolicyGroupForm([{ id: 1 } as StoragePolicy]),
			name: "Group",
		};

		expect(validatePolicyGroupForm(form, 1, t)).toBeNull();
		expect(
			validatePolicyGroupForm(
				{ ...form, categoryRestrictionEnabled: true },
				1,
				t,
			),
		).toBe("policy_group_allowed_categories_required");
		expect(
			validatePolicyGroupForm(
				{
					...form,
					categoryRestrictionEnabled: true,
					admission: { ...form.admission, allowed_categories: ["image"] },
				},
				1,
				t,
			),
		).toBeNull();
	});

	it("serializes unrestricted mode as an empty category allowlist", () => {
		const form = getDefaultPolicyGroupForm([{ id: 1 } as StoragePolicy]);
		const payload = buildPolicyGroupPayload({
			...form,
			categoryRestrictionEnabled: false,
			admission: { ...form.admission, allowed_categories: ["image", "video"] },
		});

		expect(payload.admission?.allowed_categories).toEqual([]);
		expect(payload.admission).not.toHaveProperty("denied_categories");
	});

	it("preserves exact byte thresholds when editing an existing group", () => {
		const preciseBytes = 12_345;

		expect(
			buildPolicyGroupPayload({
				name: "Tiered",
				description: "",
				isEnabled: true,
				isDefault: false,
				categoryRestrictionEnabled: false,
				items: [
					{
						key: "a",
						name: "Exact bytes",
						description: "",
						isEnabled: true,
						extensions: [],
						compoundExtensions: [],
						extensionless: null,
						categories: [],
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
				max_file_size: 99,
			},
			categoryRestrictionEnabled: true,
			executionPreference: "force_server_stream",
			items: [
				{
					key: "weighted-rule",
					name: "Weighted",
					description: "",
					isEnabled: true,
					extensions: [],
					compoundExtensions: [],
					extensionless: null,
					categories: [],
					minFileSizeMb: "",
					maxFileSizeMb: "",
					selectionMode: "weighted_random",
					unavailableBehavior: "reject",
					targets: [
						{
							key: "wr-1",
							policyId: "1",
							weight: "70",
							acceptingNewWrites: true,
						},
						{
							key: "wr-2",
							policyId: "2",
							weight: "30",
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
