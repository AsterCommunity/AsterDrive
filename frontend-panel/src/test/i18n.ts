import type { TFunction } from "i18next";

export function createMockTWithInterpolation(
	translations: Record<string, string> = {},
) {
	return ((key: string, options?: Record<string, unknown>) => {
		const namespace = typeof options?.ns === "string" ? options.ns : "admin";
		const translated = translations[`${namespace}:${key}`] ?? translations[key];
		if (translated) {
			return translated.replace(/\{\{\s*(\w+)\s*\}\}/g, (match, param) => {
				const value = options?.[param];
				return value === undefined || value === null ? match : String(value);
			});
		}
		return typeof options?.defaultValue === "string"
			? options.defaultValue
			: key;
	}) as unknown as TFunction;
}
