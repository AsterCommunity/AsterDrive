import type { i18n as I18n } from "i18next";
import { getActiveDisplayTimeZone } from "@/stores/displayTimeZoneStore";

const INTEGER_FORMATTER = new Intl.NumberFormat();

type DateFormatI18n = Pick<I18n, "language" | "resolvedLanguage" | "t">;

function getDateLocale(i18n?: DateFormatI18n): string | undefined {
	return i18n?.resolvedLanguage ?? (i18n?.language || undefined);
}

function getTimeZoneFormatOptions(
	options?: Intl.DateTimeFormatOptions,
): Intl.DateTimeFormatOptions {
	return {
		...options,
		timeZone: getActiveDisplayTimeZone(),
	};
}

function getDateTimeFormatOptions(
	options?: Intl.DateTimeFormatOptions,
): Intl.DateTimeFormatOptions {
	return getTimeZoneFormatOptions({
		...options,
		hour12: false,
		hourCycle: "h23",
	});
}

function normalizeUtcOffsetLabel(label: string): string {
	if (label === "GMT" || label === "UTC") {
		return "UTC+00:00";
	}

	const matched = label.match(/^(?:GMT|UTC)([+-])(\d{1,2})(?::?(\d{2}))?$/);
	if (!matched) {
		return label.replace(/^GMT/, "UTC");
	}

	const [, sign, hours, minutes] = matched;
	return `UTC${sign}${hours.padStart(2, "0")}:${(minutes ?? "00").padStart(2, "0")}`;
}

function formatUtcOffset(date: Date): string {
	const timeZone = getActiveDisplayTimeZone();

	for (const timeZoneName of ["longOffset", "shortOffset"] as const) {
		try {
			const parts = new Intl.DateTimeFormat("en-US", {
				timeZone,
				timeZoneName,
			}).formatToParts(date);
			const label = parts.find((part) => part.type === "timeZoneName")?.value;
			if (label) {
				return normalizeUtcOffsetLabel(label);
			}
		} catch {
			// ignore unsupported timeZoneName variants and keep the fallback below
		}
	}

	return "UTC+00:00";
}

function translateRelativeDate(
	i18n: DateFormatI18n | undefined,
	key: string,
	fallback: string,
	count?: number,
): string {
	if (!i18n) {
		return fallback;
	}

	if (count === undefined) {
		return i18n.t(key);
	}

	return i18n.t(key, { count });
}

export function formatBytes(bytes: number): string {
	if (bytes === 0) return "0 B";
	const k = 1024;
	const sizes = ["B", "KB", "MB", "GB", "TB"];
	const i = Math.floor(Math.log(bytes) / Math.log(k));
	return `${(bytes / k ** i).toFixed(1)} ${sizes[i]}`;
}

export function formatBytesPerSecond(bytesPerSecond: number): string {
	if (!Number.isFinite(bytesPerSecond) || bytesPerSecond < 0) {
		return "";
	}
	if (bytesPerSecond < 1024) {
		return `${Math.round(bytesPerSecond)} B/s`;
	}
	return `${formatBytes(bytesPerSecond)}/s`;
}

export function formatNumber(value: number): string {
	if (!Number.isFinite(value)) {
		return String(value);
	}
	return INTEGER_FORMATTER.format(value);
}

export function formatDate(dateStr: string, i18n?: DateFormatI18n): string {
	const date = new Date(dateStr);
	const now = new Date();
	const diff = now.getTime() - date.getTime();
	const minutes = Math.floor(diff / 60000);
	if (minutes < 1) {
		return translateRelativeDate(
			i18n,
			"core:date_relative_just_now",
			"just now",
		);
	}
	if (minutes < 60) {
		return translateRelativeDate(
			i18n,
			"core:date_relative_minutes_ago",
			`${minutes}m ago`,
			minutes,
		);
	}
	const hours = Math.floor(minutes / 60);
	if (hours < 24) {
		return translateRelativeDate(
			i18n,
			"core:date_relative_hours_ago",
			`${hours}h ago`,
			hours,
		);
	}
	const days = Math.floor(hours / 24);
	if (days < 30) {
		return translateRelativeDate(
			i18n,
			"core:date_relative_days_ago",
			`${days}d ago`,
			days,
		);
	}
	return date.toLocaleDateString(
		getDateLocale(i18n),
		getTimeZoneFormatOptions(),
	);
}

export function formatDateUntil(
	dateStr: string,
	i18n?: DateFormatI18n,
): string {
	const date = new Date(dateStr);
	const now = new Date();
	const diff = date.getTime() - now.getTime();
	if (diff <= 0) {
		return translateRelativeDate(i18n, "core:expired", "expired");
	}

	const minutes = Math.ceil(diff / 60000);
	if (minutes < 60) {
		return translateRelativeDate(
			i18n,
			"core:date_relative_minutes_later",
			`in ${minutes}m`,
			minutes,
		);
	}

	const hours = Math.ceil(diff / 3600000);
	if (hours < 24) {
		return translateRelativeDate(
			i18n,
			"core:date_relative_hours_later",
			`in ${hours}h`,
			hours,
		);
	}

	const days = Math.ceil(diff / 86400000);
	return translateRelativeDate(
		i18n,
		"core:date_relative_days_later",
		`in ${days}d`,
		days,
	);
}

export function formatDateAbsolute(dateStr: string): string {
	return new Date(dateStr).toLocaleString(
		undefined,
		getDateTimeFormatOptions(),
	);
}

export function formatDateAbsoluteWithOffset(dateStr: string): string {
	return `${formatDateAbsolute(dateStr)} ${formatUtcOffset(new Date(dateStr))}`;
}

export function formatDateShort(dateStr: string): string {
	return new Date(dateStr).toLocaleDateString(
		undefined,
		getTimeZoneFormatOptions(),
	);
}

export function formatDateTime(dateStr: string): string {
	return new Date(dateStr).toLocaleString(
		undefined,
		getDateTimeFormatOptions(),
	);
}

export function formatDateTimeWithOffset(dateStr: string): string {
	return `${formatDateTime(dateStr)} ${formatUtcOffset(new Date(dateStr))}`;
}
