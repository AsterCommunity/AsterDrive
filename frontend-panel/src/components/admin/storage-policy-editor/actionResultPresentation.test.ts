import { describe, expect, it } from "vitest";
import type { StorageConnectorActionDescriptor } from "@/types/api";
import { presentStorageConnectorActionOutput } from "./actionResultPresentation";

const action: StorageConnectorActionDescriptor = {
	action_id: "plugin.configure_remote",
	description_key: "configure_desc",
	endpoints: ["execute_draft_storage_policy_action"],
	fields: [],
	kind: "custom",
	label_key: "configure",
	mutates_remote_state: true,
	output_fields: [
		{
			label_key: "request_id",
			name: "request_id",
			value_kind: "text",
		},
		{
			label_key: "changed",
			name: "changed",
			value_kind: "boolean",
			true_key: "yes",
			false_key: "no",
		},
		{
			label_key: "count",
			name: "count",
			value_kind: "number",
		},
		{
			label_key: "origins",
			name: "origins",
			value_kind: "string_list",
		},
	],
	requires_authorization: false,
	requires_confirmation: true,
	requires_saved_policy: false,
};

describe("presentStorageConnectorActionOutput", () => {
	it("formats only descriptor-declared structured output", () => {
		expect(
			presentStorageConnectorActionOutput(
				action,
				{
					action_id: "plugin.configure_remote",
					ok: true,
					output: {
						changed: true,
						count: 2,
						ignored_secret: "hidden",
						origins: ["https://a.example", "https://b.example"],
						request_id: " req-1 ",
					},
				},
				(key) => `translated:${key}`,
			),
		).toEqual([
			{ label: "translated:request_id", value: "req-1" },
			{ label: "translated:changed", value: "translated:yes" },
			{ label: "translated:count", value: "2" },
			{
				label: "translated:origins",
				value: "https://a.example, https://b.example",
			},
		]);
		expect(
			presentStorageConnectorActionOutput(
				action,
				{
					action_id: "plugin.configure_remote",
					ok: true,
					output: { changed: false },
				},
				(key) => `translated:${key}`,
			),
		).toEqual([{ label: "translated:changed", value: "translated:no" }]);
	});

	it("drops absent, malformed, failed, and cross-action output", () => {
		const translate = (key: string) => key;
		expect(
			presentStorageConnectorActionOutput(
				action,
				{
					action_id: "plugin.configure_remote",
					ok: true,
					output: { changed: "yes", count: "2", origins: ["ok", 3] },
				},
				translate,
			),
		).toEqual([]);
		expect(
			presentStorageConnectorActionOutput(
				action,
				{ action_id: "plugin.other", ok: true, output: { request_id: "req" } },
				translate,
			),
		).toEqual([]);
		expect(
			presentStorageConnectorActionOutput(
				action,
				{
					action_id: "plugin.configure_remote",
					ok: false,
					output: { request_id: "req" },
				},
				translate,
			),
		).toEqual([]);
		expect(
			presentStorageConnectorActionOutput(
				{ ...action, output_fields: undefined },
				{
					action_id: "plugin.configure_remote",
					ok: true,
					output: { request_id: "req" },
				},
				translate,
			),
		).toEqual([]);
		expect(
			presentStorageConnectorActionOutput(
				{
					...action,
					output_fields: [
						{ label_key: "changed", name: "changed", value_kind: "boolean" },
						{
							label_key: "origins",
							name: "origins",
							value_kind: "string_list",
						},
					],
				},
				{
					action_id: "plugin.configure_remote",
					ok: true,
					output: { changed: true, origins: [" "] },
				},
				translate,
			),
		).toEqual([]);
		expect(
			presentStorageConnectorActionOutput(
				action,
				{
					action_id: "plugin.configure_remote",
					ok: true,
					output: [] as never,
				},
				translate,
			),
		).toEqual([]);
	});
});
