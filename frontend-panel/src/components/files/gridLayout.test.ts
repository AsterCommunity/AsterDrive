import { describe, expect, it } from "vitest";
import {
	GRID_GAP,
	GRID_ITEM_MAX_WIDTH,
	getGridColumnCount,
	getGridTemplateColumns,
} from "@/components/files/gridLayout";

describe("getGridColumnCount", () => {
	it("falls back to a single column for non-positive or non-finite widths", () => {
		expect(getGridColumnCount(0)).toBe(1);
		expect(getGridColumnCount(-10)).toBe(1);
		expect(getGridColumnCount(Number.NaN)).toBe(1);
		expect(getGridColumnCount(Number.POSITIVE_INFINITY)).toBe(1);
	});

	it("splits into a new column exactly when cards would exceed the max width", () => {
		// n 列能容纳的最大容器宽度：n * maxWidth + (n - 1) * gap，
		// 此时单卡正好 maxWidth；再宽 1px 就必须裂成 n + 1 列。
		const maxWidthForColumns = (n: number) =>
			n * GRID_ITEM_MAX_WIDTH + (n - 1) * GRID_GAP;

		for (let n = 1; n <= 24; n++) {
			expect(getGridColumnCount(maxWidthForColumns(n))).toBe(n);
			expect(getGridColumnCount(maxWidthForColumns(n) + 1)).toBe(n + 1);
		}
	});

	it("never lets card width exceed the max width", () => {
		for (let width = 100; width <= 5000; width += 7) {
			const columns = getGridColumnCount(width);
			const cardWidth = (width - (columns - 1) * GRID_GAP) / columns;
			expect(cardWidth).toBeLessThanOrEqual(GRID_ITEM_MAX_WIDTH);
		}
	});

	it("matches the intended device layouts", () => {
		// 手机竖屏（约 375px 视口去掉 padding 的内容区）：2 列
		expect(getGridColumnCount(343)).toBe(2);
		// 1080p 全屏内容区：6 列（设计基准，单卡约 250px）
		expect(getGridColumnCount(1560)).toBe(6);
	});
});

describe("getGridTemplateColumns", () => {
	it("generates a minmax(0, 1fr) template for the given column count", () => {
		expect(getGridTemplateColumns(6)).toBe("repeat(6, minmax(0, 1fr))");
		expect(getGridTemplateColumns(1)).toBe("repeat(1, minmax(0, 1fr))");
	});

	it("clamps non-positive counts to a single column", () => {
		expect(getGridTemplateColumns(0)).toBe("repeat(1, minmax(0, 1fr))");
	});
});
