import { cn } from "@/lib/utils";

interface FolderGlyphProps {
	className?: string;
}

/**
 * 网格视图文件夹图形：填充式双色（背板 + 前面板），替代线性 lucide Folder。
 * 网格中最高频的元素，值得一个真正的图形而不是带框徽章。
 * 颜色走 Tailwind fill 类，亮暗主题各自取色。
 */
export function FolderGlyph({ className }: FolderGlyphProps) {
	return (
		<svg
			viewBox="0 0 64 56"
			className={cn("size-16", className)}
			aria-hidden="true"
			focusable="false"
		>
			{/* 背板（含 tab） */}
			<path
				d="M10 4 H20 a3 3 0 0 1 2.4 1.2 L27 10.8 a3 3 0 0 0 2.4 1.2 H54 a8 8 0 0 1 8 8 V42 a8 8 0 0 1-8 8 H10 a8 8 0 0 1-8-8 V12 a8 8 0 0 1 8-8 z"
				className="fill-amber-500 dark:fill-amber-600"
			/>
			{/* 前面板 */}
			<path
				d="M2 18 H62 V42 a8 8 0 0 1-8 8 H10 a8 8 0 0 1-8-8 z"
				className="fill-amber-400 dark:fill-amber-500"
			/>
			{/* 前面板顶部高光 */}
			<path
				d="M4 19.25 H60"
				fill="none"
				stroke="currentColor"
				strokeWidth={1.5}
				strokeLinecap="round"
				className="text-white/40 dark:text-white/25"
			/>
		</svg>
	);
}
