import { useEffect, useRef } from "react";
import {
	isImeComposingKeyEvent,
	shouldIgnoreKeyboardTarget,
} from "@/lib/keyboard";
import { useFileStore } from "@/stores/fileStore";
import type { SelectionItemKey } from "@/stores/fileStore/selectionRange";

interface UseFileListKeyboardNavigationOptions {
	/** 网格列数：上下方向键按整列数移动；列表传 1 */
	columnCount: number;
	/** 是否响应左右方向键（列表视图传 false） */
	horizontal?: boolean;
	enabled?: boolean;
	/** 方向键/Shift+方向键移动后，让焦点项滚动进视口 */
	scrollToItem?: (key: SelectionItemKey) => void;
	/** Enter 打开焦点项（调用方自行处理 readOnly/trashMode 等约束） */
	onOpenFocused?: (key: SelectionItemKey) => void;
}

/**
 * 文件列表的 Finder/Explorer 式键盘导航：
 *
 * - 方向键：移动单选（grid 上下按列数跳行，list 上下逐项），锚点跟随
 * - Shift+方向键：锚点固定，选择范围扩展到移动后的焦点
 * - Enter：恰好单选时打开焦点项
 *
 * 监听挂在 document 上，但输入框/可编辑元素、IME 输入中、
 * dialog/menu/listbox 内部焦点、以及任何 mod 组合键都会被忽略，
 * 不与预览、右键菜单、系统快捷键抢按键。
 */
export function useFileListKeyboardNavigation({
	columnCount,
	horizontal = true,
	enabled = true,
	scrollToItem,
	onOpenFocused,
}: UseFileListKeyboardNavigationOptions) {
	const optionsRef = useRef({
		columnCount,
		horizontal,
		scrollToItem,
		onOpenFocused,
	});
	useEffect(() => {
		optionsRef.current = {
			columnCount,
			horizontal,
			scrollToItem,
			onOpenFocused,
		};
	});

	useEffect(() => {
		if (!enabled) return;

		function arrowDelta(key: string): number | null {
			const { columnCount: columns, horizontal: allowHorizontal } =
				optionsRef.current;
			switch (key) {
				case "ArrowLeft":
					return allowHorizontal ? -1 : null;
				case "ArrowRight":
					return allowHorizontal ? 1 : null;
				case "ArrowUp":
					return -columns;
				case "ArrowDown":
					return columns;
				default:
					return null;
			}
		}

		function handleKeyDown(event: KeyboardEvent) {
			if (
				shouldIgnoreKeyboardTarget(event.target) ||
				isImeComposingKeyEvent(event)
			) {
				return;
			}
			// 焦点在 dialog/menu/listbox 内时不劫持（预览、右键菜单有自己的键盘导航）
			if (
				event.target instanceof HTMLElement &&
				event.target.closest(
					'[role="dialog"], [role="alertdialog"], [role="menu"], [role="listbox"]',
				)
			) {
				return;
			}
			// mod 组合键保留给系统/浏览器/其他快捷键
			if (event.metaKey || event.ctrlKey || event.altKey) return;

			const delta = arrowDelta(event.key);
			if (delta !== null) {
				event.preventDefault();
				if (event.shiftKey) {
					useFileStore.getState().extendSelectionBy(delta);
				} else {
					useFileStore.getState().moveSelectionBy(delta);
				}
				// 方向键导航后 DOM 焦点与选择解耦：点击卡片留下的焦点如果还在
				// 某个条目上，后续 Enter 会被该卡片的原生 handler 捕获、错误地
				// 打开旧焦点项。清掉它，让 Enter 统一走本 hook 的 selectionFocus。
				if (
					document.activeElement instanceof HTMLElement &&
					document.activeElement.closest("[data-file-list-item]")
				) {
					document.activeElement.blur();
				}
				const focus = useFileStore.getState().selectionFocus;
				if (focus) optionsRef.current.scrollToItem?.(focus);
				return;
			}

			if (event.key === "Enter" && !event.shiftKey) {
				// 焦点在可交互元素上时让原生行为处理（卡片 Enter 已会触发打开）
				if (
					event.target instanceof HTMLElement &&
					event.target.closest('button, a, [role="button"]')
				) {
					return;
				}
				const store = useFileStore.getState();
				if (store.selectionCount() !== 1) return;
				const focus = store.selectionFocus;
				if (!focus) return;
				event.preventDefault();
				optionsRef.current.onOpenFocused?.(focus);
			}
		}

		document.addEventListener("keydown", handleKeyDown);
		return () => document.removeEventListener("keydown", handleKeyDown);
	}, [enabled]);
}
