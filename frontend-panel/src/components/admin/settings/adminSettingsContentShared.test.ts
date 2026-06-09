import { describe, expect, it } from "vitest";
import {
	buildDraftValues,
	type ConfigDraftValue,
	configDraftValueChanged,
	configDraftValuesEqual,
	configValueToString,
} from "@/components/admin/settings/adminSettingsContentShared";
import type { SystemConfig } from "@/types/api";

function createConfig(
	value: unknown,
	overrides: Partial<SystemConfig> = {},
): SystemConfig {
	return {
		category: "auth",
		description: "",
		is_sensitive: false,
		key: "auth_access_token_ttl_secs",
		requires_restart: false,
		source: "system",
		updated_at: "2026-01-01T00:00:00Z",
		value: value as SystemConfig["value"],
		value_type: "number",
		...overrides,
	};
}

describe("admin settings content shared draft values", () => {
	it("preserves scalar backend values while stringifying at UI comparison boundaries", () => {
		const numericConfig = createConfig(20);
		const booleanConfig = createConfig(false, {
			key: "storage_enabled",
			value_type: "boolean",
		});

		expect(configValueToString(20 as unknown as ConfigDraftValue)).toBe("20");
		expect(configValueToString(false as unknown as ConfigDraftValue)).toBe(
			"false",
		);
		expect(
			configDraftValuesEqual("20", 20 as unknown as ConfigDraftValue),
		).toBe(true);
		expect(
			configDraftValuesEqual("false", false as unknown as ConfigDraftValue),
		).toBe(true);
		expect(configDraftValueChanged(numericConfig, "20")).toBe(false);
		expect(configDraftValueChanged(booleanConfig, "false")).toBe(false);
		expect(buildDraftValues([numericConfig, booleanConfig])).toEqual({
			auth_access_token_ttl_secs: 20,
			storage_enabled: false,
		});
	});
});
