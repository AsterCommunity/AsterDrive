import type { i18n as I18n } from "i18next";
import { afterEach, describe, expect, it, vi } from "vitest";
import {
	formatBytes,
	formatBytesPerSecond,
	formatDate,
	formatDateAbsolute,
	formatDateAbsoluteWithOffset,
	formatDateShort,
	formatDateTime,
	formatDateTimeWithOffset,
	formatDateUntil,
	formatNumber,
} from "@/lib/format";
import {
	DISPLAY_TIME_ZONE_BROWSER,
	getActiveDisplayTimeZone,
	useDisplayTimeZoneStore,
} from "@/stores/displayTimeZoneStore";

function createI18nStub(
	language: "en" | "zh",
): Pick<I18n, "language" | "resolvedLanguage" | "t"> {
	return {
		language,
		resolvedLanguage: language,
		t: (key: string, options?: Record<string, unknown>) => {
			const count = Number(options?.count ?? 0);

			switch (key) {
				case "core:date_relative_just_now":
					return language === "zh" ? "刚刚" : "just now";
				case "core:date_relative_minutes_ago":
					return language === "zh" ? `${count}分钟前` : `${count}m ago`;
				case "core:date_relative_hours_ago":
					return language === "zh" ? `${count}小时前` : `${count}h ago`;
				case "core:date_relative_days_ago":
					return language === "zh" ? `${count}天前` : `${count}d ago`;
				case "core:date_relative_minutes_later":
					return language === "zh" ? `${count}分钟后` : `in ${count}m`;
				case "core:date_relative_hours_later":
					return language === "zh" ? `${count}小时后` : `in ${count}h`;
				case "core:date_relative_days_later":
					return language === "zh" ? `${count}天后` : `in ${count}d`;
				case "core:expired":
					return language === "zh" ? "已过期" : "Expired";
				default:
					return key;
			}
		},
	};
}

function getExpectedUtcOffset(dateStr: string, timeZone: string): string {
	const date = new Date(dateStr);
	const parts = new Intl.DateTimeFormat("en-US", {
		timeZone,
		timeZoneName: "longOffset",
	}).formatToParts(date);
	const label = parts.find((part) => part.type === "timeZoneName")?.value;
	if (!label || label === "GMT") {
		return "UTC+00:00";
	}

	const matched = label.match(/^GMT([+-])(\d{2}):(\d{2})$/);
	if (!matched) {
		return label.replace(/^GMT/, "UTC");
	}

	const [, sign, hours, minutes] = matched;
	return `UTC${sign}${hours}:${minutes}`;
}

describe("format helpers", () => {
	afterEach(() => {
		vi.useRealTimers();
		useDisplayTimeZoneStore
			.getState()
			._applyFromServer(DISPLAY_TIME_ZONE_BROWSER);
		localStorage.removeItem("aster-display-time-zone");
	});

	it("formats byte sizes", () => {
		expect(formatBytes(0)).toBe("0 B");
		expect(formatBytes(1536)).toBe("1.5 KB");
		expect(formatBytes(1024 * 1024)).toBe("1.0 MB");
		expect(formatBytes(-1)).toBe("0 B");
		expect(formatBytes(Number.NaN)).toBe("0 B");
		expect(formatBytes(Number.POSITIVE_INFINITY)).toBe("0 B");
	});

	it("formats transfer speeds with per-second suffixes", () => {
		expect(formatBytesPerSecond(0)).toBe("0 B/s");
		expect(formatBytesPerSecond(512)).toBe("512 B/s");
		expect(formatBytesPerSecond(1536)).toBe("1.5 KB/s");
	});

	it("formats integers with locale separators", () => {
		expect(formatNumber(0)).toBe(new Intl.NumberFormat().format(0));
		expect(formatNumber(4152537914)).toBe(
			new Intl.NumberFormat().format(4152537914),
		);
	});

	it("formats relative dates across time ranges", () => {
		vi.useFakeTimers();
		vi.setSystemTime(new Date("2026-03-28T12:00:00Z"));
		const enI18n = createI18nStub("en");
		const zhI18n = createI18nStub("zh");

		expect(formatDate("2026-03-28T11:59:40Z", enI18n)).toBe("just now");
		expect(formatDate("2026-03-28T11:55:00Z", enI18n)).toBe("5m ago");
		expect(formatDate("2026-03-28T10:00:00Z", enI18n)).toBe("2h ago");
		expect(formatDate("2026-03-25T12:00:00Z", enI18n)).toBe("3d ago");
		expect(formatDate("2026-03-28T11:59:40Z", zhI18n)).toBe("刚刚");
		expect(formatDate("2026-03-28T11:55:00Z", zhI18n)).toBe("5分钟前");
		expect(formatDate("2026-03-28T10:00:00Z", zhI18n)).toBe("2小时前");
		expect(formatDate("2026-03-25T12:00:00Z", zhI18n)).toBe("3天前");
	});

	it("formats future relative dates until expiry", () => {
		vi.useFakeTimers();
		vi.setSystemTime(new Date("2026-03-28T12:00:00Z"));
		const enI18n = createI18nStub("en");
		const zhI18n = createI18nStub("zh");

		expect(formatDateUntil("2026-03-28T12:00:20Z", enI18n)).toBe("in 1m");
		expect(formatDateUntil("2026-03-28T12:05:00Z", enI18n)).toBe("in 5m");
		expect(formatDateUntil("2026-03-28T14:00:00Z", enI18n)).toBe("in 2h");
		expect(formatDateUntil("2026-04-04T12:00:00Z", enI18n)).toBe("in 7d");
		expect(formatDateUntil("2026-03-28T12:05:00Z", zhI18n)).toBe("5分钟后");
		expect(formatDateUntil("2026-03-28T14:00:00Z", zhI18n)).toBe("2小时后");
		expect(formatDateUntil("2026-04-04T12:00:00Z", zhI18n)).toBe("7天后");
		expect(formatDateUntil("2026-03-28T11:59:00Z", zhI18n)).toBe("已过期");
	});

	it("uses the i18n locale when formatDate falls back to calendar dates", () => {
		vi.useFakeTimers();
		vi.setSystemTime(new Date("2026-03-28T12:00:00Z"));
		const value = "2026-02-25T12:00:00Z";

		expect(formatDate(value, createI18nStub("en"))).toBe(
			new Date(value).toLocaleDateString("en"),
		);
		expect(formatDate(value, createI18nStub("zh"))).toBe(
			new Date(value).toLocaleDateString("zh"),
		);
	});

	it("falls back to stable English relative strings when i18n is omitted", () => {
		vi.useFakeTimers();
		vi.setSystemTime(new Date("2026-03-28T12:00:00Z"));
		const value = "2026-02-25T12:00:00Z";

		expect(formatDate("2026-03-28T11:59:40Z")).toBe("just now");
		expect(formatDate("2026-03-28T11:55:00Z")).toBe("5m ago");
		expect(formatDate("2026-03-28T10:00:00Z")).toBe("2h ago");
		expect(formatDate("2026-03-25T12:00:00Z")).toBe("3d ago");
		expect(formatDateUntil("2026-03-28T12:05:00Z")).toBe("in 5m");
		expect(formatDateUntil("2026-03-28T14:00:00Z")).toBe("in 2h");
		expect(formatDateUntil("2026-04-04T12:00:00Z")).toBe("in 7d");
		expect(formatDateUntil("2026-03-28T11:59:00Z")).toBe("expired");
		expect(formatDate(value)).toBe(
			new Date(value).toLocaleDateString(undefined, {
				timeZone: getActiveDisplayTimeZone(),
			}),
		);
	});

	it("delegates absolute date formatting to the built-in locale helpers", () => {
		const value = "2026-03-28T12:34:56Z";
		const activeTimeZone = getActiveDisplayTimeZone();

		expect(formatDateAbsolute(value)).toBe(
			new Date(value).toLocaleString(undefined, {
				hour12: false,
				hourCycle: "h23",
				timeZone: activeTimeZone,
			}),
		);
		expect(formatDateShort(value)).toBe(
			new Date(value).toLocaleDateString(undefined, {
				timeZone: activeTimeZone,
			}),
		);
		expect(formatDateTime(value)).toBe(
			new Date(value).toLocaleString(undefined, {
				hour12: false,
				hourCycle: "h23",
				timeZone: activeTimeZone,
			}),
		);
		expect(formatDateAbsoluteWithOffset(value)).toBe(
			`${new Date(value).toLocaleString(undefined, {
				hour12: false,
				hourCycle: "h23",
				timeZone: activeTimeZone,
			})} ${getExpectedUtcOffset(value, activeTimeZone)}`,
		);
		expect(formatDateTimeWithOffset(value)).toBe(
			`${new Date(value).toLocaleString(undefined, {
				hour12: false,
				hourCycle: "h23",
				timeZone: activeTimeZone,
			})} ${getExpectedUtcOffset(value, activeTimeZone)}`,
		);
	});

	it("formats calendar and absolute dates using the selected display time zone", () => {
		vi.useFakeTimers();
		vi.setSystemTime(new Date("2026-04-05T12:00:00Z"));
		useDisplayTimeZoneStore.getState()._applyFromServer("America/Los_Angeles");
		const value = "2026-03-01T01:30:00Z";

		expect(formatDate(value, createI18nStub("en"))).toBe(
			new Date(value).toLocaleDateString("en", {
				timeZone: "America/Los_Angeles",
			}),
		);
		expect(formatDateAbsolute(value)).toBe(
			new Date(value).toLocaleString(undefined, {
				hour12: false,
				hourCycle: "h23",
				timeZone: "America/Los_Angeles",
			}),
		);
		expect(formatDateShort(value)).toBe(
			new Date(value).toLocaleDateString(undefined, {
				timeZone: "America/Los_Angeles",
			}),
		);
		expect(formatDateTime(value)).toBe(
			new Date(value).toLocaleString(undefined, {
				hour12: false,
				hourCycle: "h23",
				timeZone: "America/Los_Angeles",
			}),
		);
		expect(formatDateAbsoluteWithOffset(value)).toBe(
			`${new Date(value).toLocaleString(undefined, {
				hour12: false,
				hourCycle: "h23",
				timeZone: "America/Los_Angeles",
			})} UTC-08:00`,
		);
		expect(formatDateTimeWithOffset(value)).toBe(
			`${new Date(value).toLocaleString(undefined, {
				hour12: false,
				hourCycle: "h23",
				timeZone: "America/Los_Angeles",
			})} UTC-08:00`,
		);
	});
});
