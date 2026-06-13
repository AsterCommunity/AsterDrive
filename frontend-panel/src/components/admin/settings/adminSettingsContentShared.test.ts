import { describe, expect, it } from "vitest";
import {
	buildDraftValues,
	type ConfigDraftValue,
	configDraftValueChanged,
	configDraftValuesEqual,
	configValueToString,
	getAvailableDisplayUnits,
	getPreferredDisplayUnit,
	TIME_DISPLAY_UNITS,
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
		expect(
			configDraftValuesEqual("true", true as unknown as ConfigDraftValue),
		).toBe(true);
		expect(configDraftValueChanged(numericConfig, "20")).toBe(false);
		expect(configDraftValueChanged(booleanConfig, "false")).toBe(false);
		expect(buildDraftValues([numericConfig, booleanConfig])).toEqual({
			auth_access_token_ttl_secs: 20,
			storage_enabled: false,
		});
	});

	it("keeps all unit choices visible while preferring divisible display units", () => {
		expect(
			getAvailableDisplayUnits(TIME_DISPLAY_UNITS.seconds, "1").map(
				(unit) => unit.value,
			),
		).toEqual(["days", "hours", "minutes", "seconds"]);
		expect(getPreferredDisplayUnit(TIME_DISPLAY_UNITS.seconds, "1").value).toBe(
			"seconds",
		);
		expect(
			getPreferredDisplayUnit(TIME_DISPLAY_UNITS.seconds, "60").value,
		).toBe("minutes");
		expect(
			getPreferredDisplayUnit(TIME_DISPLAY_UNITS.seconds, "3600").value,
		).toBe("hours");
		expect(getPreferredDisplayUnit(TIME_DISPLAY_UNITS.seconds, "").value).toBe(
			"seconds",
		);
	});

	it("falls back to the smallest preferred unit for zero, invalid, and non-divisible values", () => {
		for (const value of ["0", "abc", "1234"]) {
			expect(
				getAvailableDisplayUnits(TIME_DISPLAY_UNITS.seconds, value).map(
					(unit) => unit.value,
				),
			).toEqual(["days", "hours", "minutes", "seconds"]);
			expect(
				getPreferredDisplayUnit(TIME_DISPLAY_UNITS.seconds, value).value,
			).toBe("seconds");
		}
	});
});
