import { z } from "zod/v4";

export type NumberUnitOption<TValue extends string = string> = {
	labelKey: string;
	multiplier: number;
	value: TValue;
};

export const nonNegativeIntegerUnitValueSchema = z.string().refine((value) => {
	const normalized = value.trim();
	if (!normalized) {
		return true;
	}
	if (!/^\d+$/.test(normalized)) {
		return false;
	}
	const parsed = Number.parseInt(normalized, 10);
	return Number.isSafeInteger(parsed);
});

export function parseNumberUnitValue(value: string) {
	const parsed = nonNegativeIntegerUnitValueSchema.safeParse(value);
	if (!parsed.success) {
		return null;
	}

	const normalized = value.trim();
	return normalized ? Number.parseInt(normalized, 10) : 0;
}

export function numberUnitValueIsValid(value: string) {
	return nonNegativeIntegerUnitValueSchema.safeParse(value).success;
}

export function convertNumberUnitValueToBaseUnit(
	value: string,
	unit: NumberUnitOption,
) {
	const parsed = parseNumberUnitValue(value);
	if (parsed === null) {
		return null;
	}
	if (!Number.isFinite(unit.multiplier) || unit.multiplier <= 0) {
		return null;
	}

	const converted = parsed * unit.multiplier;
	return Number.isSafeInteger(converted) && converted >= 0 ? converted : null;
}
