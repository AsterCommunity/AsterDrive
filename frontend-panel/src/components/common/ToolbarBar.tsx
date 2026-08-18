import type { ReactNode } from "react";

interface ToolbarBarProps {
	left: ReactNode;
	right?: ReactNode;
}

/**
 * D9 去框化：工具行不再有底部横线、色带、胶囊描边和按钮组竖线，
 * 面包屑与操作直接坐在页面背景上，分区交给间距。
 * 左 padding 与内容区（FileGrid `px-4 md:p-5`）对齐，保持纵向排版线一致。
 */
export function ToolbarBar({ left, right }: ToolbarBarProps) {
	return (
		<div className="px-4 py-2 sm:py-2.5 md:px-5">
			<div className="flex h-9 min-w-0 items-center gap-1.5 sm:h-10 sm:gap-2">
				<div className="flex min-w-0 flex-1 items-center gap-1.5 sm:gap-2">
					{left}
				</div>
				{right && (
					<div className="flex shrink-0 items-center gap-1 sm:gap-2">
						{right}
					</div>
				)}
			</div>
		</div>
	);
}
