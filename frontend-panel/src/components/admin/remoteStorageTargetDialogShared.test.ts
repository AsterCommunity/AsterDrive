import { describe, expect, it } from "vitest";
import {
	buildCreateRemoteStorageTargetPayload,
	buildUpdateRemoteStorageTargetPayload,
	createRemoteStorageTargetForm,
	getRemoteStorageTargetForm,
} from "@/components/admin/remoteStorageTargetDialogShared";
import type {
	RemoteStorageTargetConnectorDescriptor,
	RemoteStorageTargetInfo,
} from "@/types/api";

const descriptor: RemoteStorageTargetConnectorDescriptor = {
	connector_id: "plugin.example.archive",
	config_schema_version: 3,
	credential_schema_version: 2,
	label_key: "archive",
	description_key: "archive_desc",
	fields: [
		{
			name: "path",
			scope: "connector_config",
			kind: "text",
			label_key: "path",
			required: true,
			secret: false,
		},
		{
			name: "enabled",
			scope: "connector_config",
			kind: "boolean",
			label_key: "enabled",
			required: false,
			secret: false,
			default_value: true,
		},
		{
			name: "limit",
			scope: "connector_config",
			kind: "number",
			label_key: "limit",
			required: false,
			secret: false,
			default_value: 5,
		},
		{
			name: "token",
			scope: "static_credential",
			kind: "secret",
			label_key: "token",
			required: true,
			secret: true,
		},
	],
};
const target: RemoteStorageTargetInfo = {
	target_key: "target",
	name: "Archive",
	connector_id: descriptor.connector_id,
	connector_config: {
		format_version: 1,
		connector_id: descriptor.connector_id,
		schema_version: 3,
		values: { path: "saved", enabled: false, limit: 9 },
	},
	credential_configured: true,
	connector_available: true,
	is_default: false,
	desired_revision: 1,
	applied_revision: 1,
	last_error: "",
	created_at: "",
	updated_at: "",
};

describe("remoteStorageTargetDialogShared", () => {
	it("applies descriptor defaults to generic create state", () => {
		expect(createRemoteStorageTargetForm(descriptor, true)).toEqual({
			name: "",
			connector_id: descriptor.connector_id,
			values: { path: "", enabled: true, limit: 5, token: "" },
			is_default: true,
		});
	});
	it("loads saved config without echoing credentials", () => {
		expect(getRemoteStorageTargetForm(target, descriptor).values).toEqual({
			path: "saved",
			enabled: false,
			limit: 9,
			token: "",
		});
	});
	it("splits config and credential maps generically", () => {
		const form = {
			name: " Archive ",
			connector_id: descriptor.connector_id,
			values: { path: " next ", enabled: true, limit: 7, token: " secret " },
			is_default: true,
		};
		expect(buildCreateRemoteStorageTargetPayload(form, descriptor)).toEqual({
			name: "Archive",
			connector_config: {
				format_version: 1,
				connector_id: descriptor.connector_id,
				schema_version: 3,
				values: { path: "next", enabled: true, limit: 7 },
			},
			credential: { mode: "static", values: { token: "secret" } },
			is_default: true,
		});
	});
	it("preserves saved credentials when same connector submits blank secret", () => {
		const form = getRemoteStorageTargetForm(target, descriptor);
		expect(
			buildUpdateRemoteStorageTargetPayload(form, descriptor, target)
				.credential,
		).toBeUndefined();
	});
	it("sends explicit credentials when switching connector", () => {
		const other = {
			...target,
			connector_id: "plugin.example.other",
			connector_config: {
				...target.connector_config,
				connector_id: "plugin.example.other",
			},
		};
		const form = {
			...getRemoteStorageTargetForm(target, descriptor),
			values: { path: "next", enabled: true, limit: 5, token: "new" },
		};
		expect(
			buildUpdateRemoteStorageTargetPayload(form, descriptor, other).credential,
		).toEqual({ mode: "static", values: { token: "new" } });
	});
});
