import { PreviewCard } from "@base-ui/react/preview-card";
import { useEffect } from "react";
import type { ThumbnailFileLike } from "@/components/files/FileThumbnail";
import { useBlobUrl } from "@/hooks/useBlobUrl";
import { supportsGeneratedThumbnailFile } from "@/lib/thumbnailSupport";
import { cn } from "@/lib/utils";
import { fileService } from "@/services/fileService";
import { useThumbnailSupportStore } from "@/stores/thumbnailSupportStore";

interface FileHoverPreviewProps {
	/** 浮层定位锚点（媒体区/名称单元格），未挂载时不渲染 */
	anchor: Element | null;
	file: ThumbnailFileLike;
	/** 由 useDelayedHoverPreview 给出的悬停意向结果 */
	open: boolean;
	onClose: () => void;
	thumbnailPath?: string;
	align?: "start" | "center" | "end";
	/**
	 * 覆盖模式（网格卡片）：底边对齐锚点底部——大图直接盖在缩略图
	 * 位置上向上展开，而不是弹到卡片外；宽度放到锚点的 1.15 倍，
	 * 左右略突出卡片边界——超宽图按比例显示后高度比缩略图还矮，
	 * 与锚点等宽会看起来像被缩略图"吞掉"，略宽一点才有展开感
	 */
	cover?: boolean;
	sideOffset?: number;
}

/**
 * 悬停意向预览：useDelayedHoverPreview 判定 ~1s 悬停后，把已加载的
 * 缩略图展开成大图。D9 无容器形态延伸到浮层——预览图本身就是内容，
 * 圆角直接裁在图上，不加边框/底色容器；cover 模式下大图原位盖住
 * 缩略图（底边停在文件名上方，不遮挡名称），Portal 渲染不受滚动
 * 容器 overflow 裁剪。
 */
export function FileHoverPreview({
	anchor,
	file,
	open,
	onClose,
	thumbnailPath,
	align = "center",
	cover = false,
	sideOffset = 4,
}: FileHoverPreviewProps) {
	const thumbnailSupport = useThumbnailSupportStore((state) => state.config);
	const thumbnailSupportLoaded = useThumbnailSupportStore(
		(state) => state.isLoaded,
	);
	const loadThumbnailSupport = useThumbnailSupportStore((state) => state.load);
	const canRequestThumbnail =
		thumbnailSupportLoaded &&
		supportsGeneratedThumbnailFile(file.name, thumbnailSupport);

	useEffect(() => {
		if (!thumbnailSupportLoaded) {
			void loadThumbnailSupport();
		}
	}, [loadThumbnailSupport, thumbnailSupportLoaded]);

	// open 前不订阅 blob：悬停意向成立时 FileThumbnail 通常已加载同一路径，
	// useBlobUrl 缓存命中零额外请求；缓存未中则等 blob 就绪后再展开
	const blobPath =
		open && canRequestThumbnail
			? (thumbnailPath ?? fileService.thumbnailPath(file.id))
			: null;
	const { blobUrl } = useBlobUrl(blobPath, { lane: "thumbnail" });

	if (!canRequestThumbnail || !anchor) {
		return null;
	}

	return (
		<PreviewCard.Root
			open={open && blobUrl !== null}
			onOpenChange={(nextOpen) => {
				if (!nextOpen) onClose();
			}}
		>
			<PreviewCard.Portal>
				<PreviewCard.Positioner
					anchor={anchor}
					side="top"
					align={align}
					sideOffset={
						cover
							? ({ anchor: anchorRect, positioner }) =>
									positioner.height < anchorRect.height
										? // 预览比锚点矮（超宽图窄条）：垂直居中于锚点，
											// 像缩略图原位放大而不是贴底"被吞掉"
											-(anchorRect.height + positioner.height) / 2
										: // 预览更高：底边对齐锚点底部，不遮挡文件名
											-anchorRect.height
							: sideOffset
					}
					// 整个浮层 pointer-through：预览纯展示不交互，且不能抢占锚点的
					// hit-test——否则浮层盖住锚点会触发 pointerleave 把预览瞬间关掉
					className="isolate z-(--z-popover) pointer-events-none"
				>
					<PreviewCard.Popup
						data-slot="file-hover-preview"
						className={cn(
							"pointer-events-none origin-(--transform-origin) duration-150 ease-out data-open:animate-in data-open:fade-in-0 data-open:zoom-in-95 data-closed:animate-out data-closed:fade-out-0 data-closed:zoom-out-95 motion-reduce:duration-0",
						)}
					>
						{blobUrl ? (
							<img
								src={blobUrl}
								alt={file.name}
								draggable={false}
								className={cn(
									"block h-auto rounded-xl object-cover shadow-xl shadow-black/15 dark:shadow-black/50",
									cover
										? // 宽度 = 锚点 ×1.15（略突出卡片），高度按图片比例，封顶 max-h-72
											"max-h-72 w-[calc(var(--anchor-width)*1.15)]"
										: "max-h-56 w-auto max-w-64",
								)}
							/>
						) : null}
					</PreviewCard.Popup>
				</PreviewCard.Positioner>
			</PreviewCard.Portal>
		</PreviewCard.Root>
	);
}
