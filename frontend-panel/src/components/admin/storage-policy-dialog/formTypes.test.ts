import { describe, expect, it } from "vitest";
import {
	connectorBooleanValue,
	connectorNumberValue,
	connectorStringValue,
	emptyForm,
	getPolicyForm,
	updatedConnectorConfigValues,
	updatedCredentialValues,
	withConnectorFormValue,
} from "./formTypes";

describe("storage policy form values", () => {
	it("reads typed connector values and applies fallbacks", () => {
		const form = {
			...emptyForm,
			connector_config_values: {
				enabled: true,
				path: "/archive",
				port: 443,
			},
		};

		expect(connectorStringValue(form, "path")).toBe("/archive");
		expect(connectorStringValue(form, "missing", "fallback")).toBe("fallback");
		expect(connectorBooleanValue(form, "enabled")).toBe(true);
		expect(connectorBooleanValue(form, "path", false)).toBe(false);
		expect(connectorNumberValue(form, "port")).toBe(443);
		expect(connectorNumberValue(form, "path")).toBeNull();
	});

	it("returns immutable connector and credential updates", () => {
		const form = {
			...emptyForm,
			connector_config_values: { endpoint: "old" },
			credential_values: { token: "old-token" },
		};

		expect(withConnectorFormValue(form, "endpoint", "new")).toMatchObject({
			connector_config_values: { endpoint: "new" },
		});
		expect(updatedConnectorConfigValues(form, "bucket", "archive")).toEqual({
			bucket: "archive",
			endpoint: "old",
		});
		expect(updatedCredentialValues(form, "token", "new-token")).toEqual({
			token: "new-token",
		});
		expect(form.connector_config_values.endpoint).toBe("old");
		expect(form.credential_values.token).toBe("old-token");
	});

	it("falls back from malformed persisted connector envelope members", () => {
		const form = getPolicyForm({
			behavior: {},
			connector_config: { connector_id: 42, values: [] },
			connector_id: "plugin.archive",
			name: "Archive",
		} as never);

		expect(form.connector_id).toBe("plugin.archive");
		expect(form.connector_config_values).toEqual({});

		const valid = getPolicyForm({
			behavior: {},
			connector_config: {
				connector_id: "plugin.persisted",
				values: { enabled: true },
			},
			connector_id: "plugin.archive",
			name: "Archive",
		} as never);
		expect(valid.connector_id).toBe("plugin.persisted");
		expect(valid.connector_config_values).toEqual({ enabled: true });
	});
});
