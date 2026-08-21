import { useCallback, useEffect, useRef, useState } from "react";

interface UseDelayedHoverPreviewOptions {
	/** 悬停多久后展开预览，默认 700ms */
	delay?: number;
}

interface UseDelayedHoverPreviewResult<T extends HTMLElement> {
	/** 绑定到 hover 触发元素（同时作为浮层定位锚点）的 callback ref */
	triggerRef: (node: T | null) => void;
	/** 当前触发元素，未挂载时为 null */
	triggerEl: T | null;
	open: boolean;
	close: () => void;
}

/**
 * Hover 意向计时：鼠标持续悬停 delay 后才打开预览，期间移出或开始拖拽即取消。
 * 只响应鼠标（pointerType === "mouse"），触摸/触控笔点按不触发——
 * 触屏 tap 紧接着就是点击导航，3s 意向计时在触屏上没有意义。
 */
export function useDelayedHoverPreview<T extends HTMLElement = HTMLElement>({
	delay = 700,
}: UseDelayedHoverPreviewOptions = {}): UseDelayedHoverPreviewResult<T> {
	const [triggerEl, setTriggerEl] = useState<T | null>(null);
	const [open, setOpen] = useState(false);
	const timerRef = useRef<ReturnType<typeof setTimeout> | null>(null);

	const clearTimer = useCallback(() => {
		if (timerRef.current !== null) {
			clearTimeout(timerRef.current);
			timerRef.current = null;
		}
	}, []);

	const close = useCallback(() => {
		clearTimer();
		setOpen(false);
	}, [clearTimer]);

	useEffect(() => {
		if (!triggerEl) return;

		const handlePointerEnter = (event: PointerEvent) => {
			if (event.pointerType !== "mouse") return;
			clearTimer();
			timerRef.current = setTimeout(() => {
				timerRef.current = null;
				setOpen(true);
			}, delay);
		};
		// HTML5 拖拽会中断 pointer 事件流（不一定再发 pointerleave），
		// 拖拽起源时主动收起，避免拖着文件还弹预览
		const handleDragStart = () => close();

		triggerEl.addEventListener("pointerenter", handlePointerEnter);
		triggerEl.addEventListener("pointerleave", close);
		triggerEl.addEventListener("dragstart", handleDragStart);

		return () => {
			triggerEl.removeEventListener("pointerenter", handlePointerEnter);
			triggerEl.removeEventListener("pointerleave", close);
			triggerEl.removeEventListener("dragstart", handleDragStart);
			clearTimer();
		};
	}, [triggerEl, delay, clearTimer, close]);

	return { triggerRef: setTriggerEl, triggerEl, open, close };
}
