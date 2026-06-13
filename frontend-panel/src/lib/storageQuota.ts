import { z } from "zod/v4";
import {
	convertNumberUnitValueToBaseUnit,
	numberUnitValueIsValid,
} from "@/lib/numberUnit";

export const BYTES_PER_MB = 1024 * 1024;
export const MAX_SAFE_STORAGE_QUOTA_MB = Math.floor(
	Number.MAX_SAFE_INTEGER / BYTES_PER_MB,
);

const STORAGE_QUOTA_UNIT_VALUES = [
	"terabytes",
	"gigabytes",
	"megabytes",
	"kilobytes",
	"bytes",
] as const;

export const STORAGE_QUOTA_UNITS = [
	{
		labelKey: "settings_size_unit_terabytes",
		multiplier: 1024 ** 4,
		value: STORAGE_QUOTA_UNIT_VALUES[0],
	},
	{
		labelKey: "settings_size_unit_gigabytes",
		multiplier: 1024 ** 3,
		value: STORAGE_QUOTA_UNIT_VALUES[1],
	},
	{
		labelKey: "settings_size_unit_megabytes",
		multiplier: BYTES_PER_MB,
		value: STORAGE_QUOTA_UNIT_VALUES[2],
	},
	{
		labelKey: "settings_size_unit_kilobytes",
		multiplier: 1024,
		value: STORAGE_QUOTA_UNIT_VALUES[3],
	},
	{
		labelKey: "settings_size_unit_bytes",
		multiplier: 1,
		value: STORAGE_QUOTA_UNIT_VALUES[4],
	},
] as const;

export type StorageQuotaUnit = (typeof STORAGE_QUOTA_UNITS)[number]["value"];

export const storageQuotaDraftSchema = z
	.object({
		unit: z.enum(STORAGE_QUOTA_UNIT_VALUES),
		value: z.string(),
	})
	.superRefine((draft, ctx) => {
		if (!numberUnitValueIsValid(draft.value)) {
			ctx.addIssue({
				code: "custom",
				path: ["value"],
			});
			return;
		}

		const unit = getStorageQuotaUnit(draft.unit);
		if (!unit || convertNumberUnitValueToBaseUnit(draft.value, unit) === null) {
			ctx.addIssue({
				code: "custom",
				path: ["value"],
			});
		}
	});

export function getStorageQuotaUnit(unitValue: StorageQuotaUnit) {
	return STORAGE_QUOTA_UNITS.find((unit) => unit.value === unitValue);
}

export function formatStorageQuotaDraft(quotaBytes: number | null | undefined) {
	const normalizedQuota = quotaBytes ?? 0;
	if (normalizedQuota <= 0) {
		return {
			unit: "megabytes" as StorageQuotaUnit,
			value: "",
		};
	}

	const unit =
		STORAGE_QUOTA_UNITS.find(
			(candidate) => normalizedQuota % candidate.multiplier === 0,
		) ?? STORAGE_QUOTA_UNITS[STORAGE_QUOTA_UNITS.length - 1];

	return {
		unit: unit.value,
		value: String(normalizedQuota / unit.multiplier),
	};
}

export function parseStorageQuotaValueToBytes(
	value: string,
	unitValue: StorageQuotaUnit,
) {
	const parsedDraft = storageQuotaDraftSchema.safeParse({
		unit: unitValue,
		value,
	});
	if (!parsedDraft.success) {
		return null;
	}

	const normalized = value.trim();
	if (!normalized) {
		return 0;
	}

	const unit = getStorageQuotaUnit(unitValue);
	if (!unit) return null;

	return convertNumberUnitValueToBaseUnit(normalized, unit);
}

export function storageQuotaDraftIsValid(
	value: string,
	unitValue: StorageQuotaUnit,
) {
	return storageQuotaDraftSchema.safeParse({
		unit: unitValue,
		value,
	}).success;
}

export function parseStorageQuotaMbToBytes(value: string) {
	const normalized = value.trim();
	if (!/^\d+$/.test(normalized)) {
		return null;
	}

	const mb = Number.parseInt(normalized, 10);
	if (
		!Number.isFinite(mb) ||
		!Number.isSafeInteger(mb) ||
		mb > MAX_SAFE_STORAGE_QUOTA_MB
	) {
		return null;
	}

	return mb * BYTES_PER_MB;
}
