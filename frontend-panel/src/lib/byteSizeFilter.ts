import type { NumberUnitOption } from "@/lib/numberUnit";

export const BYTE_FILTER_UNITS = [
	{ value: "B", multiplier: 1, labelKey: "filter_unit_b" },
	{ value: "KB", multiplier: 1024, labelKey: "filter_unit_kb" },
	{ value: "MB", multiplier: 1024 ** 2, labelKey: "filter_unit_mb" },
	{ value: "GB", multiplier: 1024 ** 3, labelKey: "filter_unit_gb" },
	{ value: "TB", multiplier: 1024 ** 4, labelKey: "filter_unit_tb" },
] as const satisfies readonly NumberUnitOption[];
export type ByteFilterUnit = (typeof BYTE_FILTER_UNITS)[number]["value"];
export function parseByteFilterUnit(value: string | null): ByteFilterUnit {
	return BYTE_FILTER_UNITS.find((unit) => unit.value === value)?.value ?? "B";
}
export function parseByteSizeInput(
	value: string,
	unit: ByteFilterUnit,
): number | undefined | null {
	const text = value.trim();
	if (!text) return undefined;
	if (!/^(?:\d+(?:\.\d*)?|\.\d+)$/.test(text) || text.length > 100) return null;
	const [whole, fraction = ""] = text.split(".");
	const denominator = 10n ** BigInt(fraction.length);
	const numerator =
		BigInt(`${whole || "0"}${fraction}`) *
		BigInt(
			BYTE_FILTER_UNITS.find((option) => option.value === unit)?.multiplier ??
				1,
		);
	if (
		numerator % denominator !== 0n ||
		numerator / denominator > BigInt(Number.MAX_SAFE_INTEGER)
	)
		return null;
	return Number(numerator / denominator);
}
export function formatByteSizeInput(
	bytes: number | undefined,
	unit: ByteFilterUnit,
): string {
	if (bytes == null) return "";
	const multiplier = BigInt(
		BYTE_FILTER_UNITS.find((option) => option.value === unit)?.multiplier ?? 1,
	);
	const integer = BigInt(bytes);
	const fraction = (((integer % multiplier) * 10n ** 40n) / multiplier)
		.toString()
		.padStart(40, "0")
		.replace(/0+$/, "");
	return `${integer / multiplier}${fraction ? `.${fraction}` : ""}`;
}
