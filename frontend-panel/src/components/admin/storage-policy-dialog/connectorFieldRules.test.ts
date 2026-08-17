import { describe, expect, it } from "vitest";
import type {
	StorageConnectorDescriptor,
	StorageConnectorFieldDescriptor,
} from "@/types/api";
import {
	applyConnectorConfigFieldTransition,
	connectorSelectOptions,
	isConnectorFieldRequired,
	isConnectorFieldVisible,
	normalizeConnectorConfigValues,
} from "./connectorFieldRules";

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

function condition(name: string, value: string) {
	return { field: name, value };
}

function oneDriveDescriptor(): StorageConnectorDescriptor {
	return {
		fields: [
			field("cloud", {
				default_value: "global",
				kind: "select",
				select: {
					options: [
						{ label_key: "global", value: "global" },
						{ label_key: "china", value: "china" },
					],
					value_kind: "string",
				},
			}),
			field("account_mode", {
				default_rules: [
					{
						conditions: [condition("cloud", "china")],
						value: "work_or_school",
					},
				],
				default_value: "personal",
				kind: "select",
				select: {
					options: [
						{
							available_when: [condition("cloud", "global")],
							label_key: "personal",
							value: "personal",
						},
						{ label_key: "work", value: "work_or_school" },
						{ label_key: "site", value: "sharepoint_site" },
						{ label_key: "group", value: "group_drive" },
					],
					value_kind: "string",
				},
			}),
			field("tenant", {
				default_rules: [
					{
						conditions: [condition("cloud", "china")],
						value: "organizations",
					},
					{
						conditions: [condition("account_mode", "personal")],
						value: "consumers",
					},
					{
						conditions: [condition("account_mode", "work_or_school")],
						value: "common",
					},
					{
						conditions: [condition("account_mode", "sharepoint_site")],
						value: "organizations",
					},
					{
						conditions: [condition("account_mode", "group_drive")],
						value: "organizations",
					},
				],
				default_value: "common",
			}),
			field("site_id", {
				inactive_value_behavior: "clear",
				required_when: [condition("account_mode", "sharepoint_site")],
				visible_when: [condition("account_mode", "sharepoint_site")],
			}),
			field("group_id", {
				inactive_value_behavior: "clear",
				required_when: [condition("account_mode", "group_drive")],
				visible_when: [condition("account_mode", "group_drive")],
			}),
		],
	} as StorageConnectorDescriptor;
}

function descriptorField(descriptor: StorageConnectorDescriptor, name: string) {
	const value = descriptor.fields.find((field) => field.name === name);
	if (!value) {
		throw new Error(`missing descriptor field: ${name}`);
	}
	return value;
}

describe("connector field rules", () => {
	it("resolves dependent defaults without making descriptor field order observable", () => {
		const descriptor = oneDriveDescriptor();
		descriptor.fields.reverse();

		expect(normalizeConnectorConfigValues({}, descriptor)).toMatchObject({
			account_mode: "personal",
			cloud: "global",
			tenant: "consumers",
		});
	});

	it("normalizes China cloud to an available account mode and follows automatic tenant defaults", () => {
		const next = applyConnectorConfigFieldTransition(
			{
				account_mode: "personal",
				cloud: "global",
				site_id: "stale-site",
				tenant: "consumers",
			},
			oneDriveDescriptor(),
			"cloud",
			"china",
		);

		expect(next).toMatchObject({
			account_mode: "work_or_school",
			cloud: "china",
			tenant: "organizations",
		});
		expect(next).not.toHaveProperty("site_id");
	});

	it("preserves a custom tenant while changing cloud or account mode", () => {
		const descriptor = oneDriveDescriptor();
		const custom = {
			account_mode: "work_or_school",
			cloud: "global",
			tenant: "contoso.onmicrosoft.com",
		};

		const china = applyConnectorConfigFieldTransition(
			custom,
			descriptor,
			"cloud",
			"china",
		);
		expect(china.tenant).toBe("contoso.onmicrosoft.com");

		const site = applyConnectorConfigFieldTransition(
			custom,
			descriptor,
			"account_mode",
			"sharepoint_site",
		);
		expect(site.tenant).toBe("contoso.onmicrosoft.com");
	});

	it("clears inactive targets and omits stale values during final normalization", () => {
		const descriptor = oneDriveDescriptor();
		const group = applyConnectorConfigFieldTransition(
			{
				account_mode: "sharepoint_site",
				cloud: "global",
				site_id: "site-1",
				tenant: "organizations",
			},
			descriptor,
			"account_mode",
			"group_drive",
		);
		expect(group).not.toHaveProperty("site_id");
		expect(group.tenant).toBe("organizations");

		const normalized = normalizeConnectorConfigValues(
			{
				account_mode: "work_or_school",
				cloud: "global",
				group_id: "stale-group",
				site_id: "stale-site",
				tenant: "common",
			},
			descriptor,
		);
		expect(normalized).not.toHaveProperty("site_id");
		expect(normalized).not.toHaveProperty("group_id");
	});

	it("filters options and evaluates conditional visibility and requiredness", () => {
		const descriptor = oneDriveDescriptor();
		const account = descriptorField(descriptor, "account_mode");
		const site = descriptorField(descriptor, "site_id");

		expect(
			connectorSelectOptions(account, { cloud: "china" }).map(
				(option) => option.value,
			),
		).not.toContain("personal");
		expect(isConnectorFieldVisible(site, { account_mode: "personal" })).toBe(
			false,
		);
		expect(
			isConnectorFieldRequired(site, { account_mode: "sharepoint_site" }),
		).toBe(true);
	});
});
