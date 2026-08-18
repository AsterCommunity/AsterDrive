/**
 * 文件网格布局常量与列数计算。
 *
 * 网格列数完全由本模块按容器宽度计算（ResizeObserver 测容器，FileGrid 用
 * `repeat(columnCount, minmax(0, 1fr))` 渲染），不依赖 CSS auto-fill——
 * 因为目标语义是"卡片宽度到上限就裂出新列"，而 auto-fill 的列数只按
 * minmax 下限判定，表达不了上限驱动的裂列：
 *
 *   n = ceil((containerWidth + gap) / (maxWidth + gap))
 *
 * 即 n 列恰好能容纳的最大容器宽度为 n * maxWidth + (n - 1) * gap，
 * 再宽 1px 就裂成 n + 1 列。由此单卡宽度恒有 cardWidth <= maxWidth
 * （证明：n >= (W + g) / (M + g)  =>  n * M >= W - (n - 1) * g）。
 * 卡片在列数不变区间内随容器弹性变宽，触顶即裂列。
 */

/** 单个网格卡片的目标最大宽度（px）。约 1080p 全屏内容区正好 6 列。 */
export const GRID_ITEM_MAX_WIDTH = 260;

/** 网格列间距（px），对应 Tailwind 的 gap-3。 */
export const GRID_GAP = 12;

/** 按容器宽度计算列数：卡片宽度超过 GRID_ITEM_MAX_WIDTH 就增加一列。 */
export function getGridColumnCount(containerWidth: number): number {
	if (!Number.isFinite(containerWidth) || containerWidth <= 0) return 1;
	return Math.max(
		1,
		Math.ceil((containerWidth + GRID_GAP) / (GRID_ITEM_MAX_WIDTH + GRID_GAP)),
	);
}

/**
 * 生成列数对应的 grid-template-columns 值。
 * minmax(0, 1fr) 而不是裸 1fr：轨道最小尺寸取 0 而非 auto，
 * 防止超长文件名把轨道撑破。
 */
export function getGridTemplateColumns(columnCount: number): string {
	return `repeat(${Math.max(1, columnCount)}, minmax(0, 1fr))`;
}
