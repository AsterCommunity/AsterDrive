import { useFileStore } from "@/stores/fileStore";
import type { SelectionItemKey } from "@/stores/fileStore/selectionRange";

/**
 * 修饰键点击选择（Finder/Explorer 行为）：
 *
 * - Cmd/Ctrl+点击：切换该项选中状态并设为锚点，不影响其他选中项
 * - Shift+点击：从锚点到该项的范围选择，锚点保持不动
 *
 * 返回 true 表示点击已被选择逻辑消费，调用方不应再触发打开/跳转。
 * 仅在 selectionEnabled 时调用；无修饰键时返回 false，走原有打开逻辑。
 */
export function applySelectionModifiers(
	event: { metaKey: boolean; ctrlKey: boolean; shiftKey: boolean },
	key: SelectionItemKey,
): boolean {
	if (event.metaKey || event.ctrlKey) {
		const store = useFileStore.getState();
		if (key.type === "file") store.toggleFileSelection(key.id);
		else store.toggleFolderSelection(key.id);
		return true;
	}
	if (event.shiftKey) {
		useFileStore.getState().selectRangeTo(key.type, key.id);
		return true;
	}
	return false;
}
