import { FolderIconRenderer } from "@/components/files/FolderIconRenderer";
import type { FolderIcon } from "@/types/api";

interface FolderGlyphProps {
	className?: string;
	icon?: FolderIcon;
}

/**
 * 网格视图文件夹图形：填充式双色（背板 + 前面板），替代线性 lucide Folder。
 * 网格中最高频的元素，值得一个真正的图形而不是带框徽章。
 * 颜色走 Tailwind fill 类，亮暗主题各自取色。
 */
export function FolderGlyph({ className, icon }: FolderGlyphProps) {
	return <FolderIconRenderer icon={icon} className={className ?? "size-16"} />;
}
