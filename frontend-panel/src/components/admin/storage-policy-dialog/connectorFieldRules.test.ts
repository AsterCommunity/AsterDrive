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

	it("reconciles chained inactive fields to a fixed point regardless of order", () => {
		const descriptor = oneDriveDescriptor();
		const site = descriptorField(descriptor, "site_id");
		const leaf = field("site_child", {
			inactive_value_behavior: "clear",
			visible_when: [condition("site_id", "site-1")],
		});
		descriptor.fields = [
			leaf,
			site,
			...descriptor.fields.filter((item) => item !== site),
		];

		const normalized = normalizeConnectorConfigValues(
			{
				account_mode: "work_or_school",
				cloud: "global",
				site_child: "stale-child",
				site_id: "site-1",
				tenant: "common",
			},
			descriptor,
		);

		expect(normalized).not.toHaveProperty("site_id");
		expect(normalized).not.toHaveProperty("site_child");
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

	it("preserves connector-declared custom select values during normalization", () => {
		const descriptor = oneDriveDescriptor();
		const tenant = descriptorField(descriptor, "tenant");
		tenant.kind = "select";
		tenant.select = {
			allow_custom_value: true,
			custom_value_label_key: "tenant_custom",
			options: [
				{ label_key: "common", value: "common" },
				{ label_key: "organizations", value: "organizations" },
				{ label_key: "consumers", value: "consumers" },
			],
			value_kind: "string",
		};

		expect(
			normalizeConnectorConfigValues(
				{
					account_mode: "work_or_school",
					cloud: "global",
					tenant: "contoso.onmicrosoft.com",
				},
				descriptor,
			).tenant,
		).toBe("contoso.onmicrosoft.com");
	});

	it("preserves an explicitly selected preset even when it equals the previous automatic default", () => {
		const descriptor = oneDriveDescriptor();
		const tenant = descriptorField(descriptor, "tenant");
		tenant.kind = "select";
		tenant.select = {
			automatic_default_label_key: "tenant_auto",
			options: [
				{ label_key: "common", value: "common" },
				{ label_key: "organizations", value: "organizations" },
				{ label_key: "consumers", value: "consumers" },
			],
			value_kind: "string",
		};

		const next = applyConnectorConfigFieldTransition(
			{
				account_mode: "personal",
				cloud: "global",
				tenant: "consumers",
			},
			descriptor,
			"account_mode",
			"work_or_school",
			new Set(["tenant"]),
		);

		expect(next.tenant).toBe("consumers");
	});

	it("removes an automatic value when its conditional default stops applying", () => {
		const descriptor = oneDriveDescriptor();
		const tenant = descriptorField(descriptor, "tenant");
		tenant.default_value = undefined;
		tenant.default_rules = [
			{
				conditions: [condition("account_mode", "personal")],
				value: "consumers",
			},
		];
		tenant.kind = "select";
		tenant.select = {
			automatic_default_label_key: "tenant_auto",
			options: [
				{ label_key: "consumers", value: "consumers" },
				{ label_key: "common", value: "common" },
			],
			value_kind: "string",
		};

		const next = applyConnectorConfigFieldTransition(
			{
				account_mode: "personal",
				cloud: "global",
				tenant: "consumers",
			},
			descriptor,
			"account_mode",
			"work_or_school",
		);

		expect(next).not.toHaveProperty("tenant");
	});

	it("reconciles unavailable select values to a default or removes them", () => {
		const descriptor = oneDriveDescriptor();
		const account = descriptorField(descriptor, "account_mode");
		account.default_rules = [];
		account.default_value = "work_or_school";

		expect(
			normalizeConnectorConfigValues(
				{ account_mode: "personal", cloud: "china" },
				descriptor,
			).account_mode,
		).toBe("work_or_school");

		account.default_value = undefined;
		const withoutDefault = normalizeConnectorConfigValues(
			{ account_mode: "personal", cloud: "china" },
			descriptor,
		);
		expect(withoutDefault).not.toHaveProperty("account_mode");
	});
});
