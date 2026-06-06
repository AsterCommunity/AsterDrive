import {
	type PointerEvent as ReactPointerEvent,
	useCallback,
	useEffect,
	useMemo,
	useRef,
	useState,
} from "react";

const MIN_ZOOM = 0.5;
const MAX_ZOOM = 5;
const ZOOM_STEP = 0.25;
const IMAGE_TRANSFORM_ORIGIN = "center center";

interface Point {
	x: number;
	y: number;
}

function clamp(value: number, min: number, max: number) {
	return Math.min(max, Math.max(min, value));
}

function distanceBetween(first: Point, second: Point) {
	return Math.hypot(first.x - second.x, first.y - second.y);
}

function midpoint(first: Point, second: Point): Point {
	return {
		x: (first.x + second.x) / 2,
		y: (first.y + second.y) / 2,
	};
}

export function useImagePreviewTransform({
	gestureSurfaceRef,
	gesturesEnabled = true,
	imageRef,
	viewportRef,
}: {
	gestureSurfaceRef?: React.RefObject<HTMLDivElement | null>;
	gesturesEnabled?: boolean;
	imageRef: React.RefObject<HTMLImageElement | null>;
	viewportRef: React.RefObject<HTMLDivElement | null>;
}) {
	const pointersRef = useRef<Map<number, Point> | null>(null);
	if (pointersRef.current === null) {
		pointersRef.current = new Map<number, Point>();
	}
	const pointers = pointersRef.current;
	const dragStartRef = useRef<{
		imageOffset: Point;
		pointer: Point;
	} | null>(null);
	const pinchStartRef = useRef<{
		center: Point;
		distance: number;
		imageOffset: Point;
		zoom: number;
	} | null>(null);
	const [zoom, setZoom] = useState(1);
	const [rotation, setRotation] = useState(0);
	const [imageOffset, setImageOffset] = useState<Point>({ x: 0, y: 0 });
	const zoomPercent = Math.round(zoom * 100);
	const canZoomOut = zoom > MIN_ZOOM;
	const canZoomIn = zoom < MAX_ZOOM;

	const clampOffset = useCallback(
		(offset: Point, targetZoom: number): Point => {
			const image = imageRef.current;
			const viewport = viewportRef.current;
			if (!image || !viewport || targetZoom <= 1) {
				return { x: 0, y: 0 };
			}

			const viewportRect = viewport.getBoundingClientRect();
			const isSideways = rotation % 180 !== 0;
			const effectiveWidth = isSideways
				? image.offsetHeight
				: image.offsetWidth;
			const effectiveHeight = isSideways
				? image.offsetWidth
				: image.offsetHeight;
			const scaledWidth = effectiveWidth * targetZoom;
			const scaledHeight = effectiveHeight * targetZoom;
			const maxX = Math.max(0, (scaledWidth - viewportRect.width) / 2);
			const maxY = Math.max(0, (scaledHeight - viewportRect.height) / 2);

			return {
				x: clamp(offset.x, -maxX, maxX),
				y: clamp(offset.y, -maxY, maxY),
			};
		},
		[imageRef, rotation, viewportRef],
	);

	const setClampedZoom = useCallback(
		(nextZoom: number, anchor?: Point) => {
			setZoom((currentZoom) => {
				const clampedZoom = clamp(nextZoom, MIN_ZOOM, MAX_ZOOM);
				setImageOffset((currentOffset) => {
					if (!anchor || currentZoom <= 0) {
						return clampOffset(currentOffset, clampedZoom);
					}

					const viewport = viewportRef.current;
					if (!viewport) {
						return clampOffset(currentOffset, clampedZoom);
					}

					const rect = viewport.getBoundingClientRect();
					const anchorFromCenter = {
						x: anchor.x - rect.left - rect.width / 2,
						y: anchor.y - rect.top - rect.height / 2,
					};
					const scaleDelta = clampedZoom / currentZoom;
					const anchoredOffset = {
						x:
							anchorFromCenter.x -
							(anchorFromCenter.x - currentOffset.x) * scaleDelta,
						y:
							anchorFromCenter.y -
							(anchorFromCenter.y - currentOffset.y) * scaleDelta,
					};
					return clampOffset(anchoredOffset, clampedZoom);
				});
				return clampedZoom;
			});
		},
		[clampOffset, viewportRef],
	);

	const resetImageTransform = useCallback(() => {
		setZoom(1);
		setImageOffset({ x: 0, y: 0 });
		setRotation((current) => Math.round(current / 360) * 360);
	}, []);

	const zoomOut = useCallback(() => {
		setClampedZoom(zoom - ZOOM_STEP);
	}, [setClampedZoom, zoom]);

	const zoomIn = useCallback(() => {
		setClampedZoom(zoom + ZOOM_STEP);
	}, [setClampedZoom, zoom]);

	const rotateRight = useCallback(() => {
		setImageOffset({ x: 0, y: 0 });
		setRotation((current) => current + 90);
	}, []);

	const imageStyle = useMemo(
		() => ({
			transform: `translate3d(${imageOffset.x}px, ${imageOffset.y}px, 0) scale(${zoom}) rotate(${rotation}deg)`,
			transformOrigin: IMAGE_TRANSFORM_ORIGIN,
			transition: pointers.size > 0 ? "none" : "transform 160ms ease-out",
		}),
		[imageOffset.x, imageOffset.y, pointers.size, rotation, zoom],
	);

	const handlePointerDown = useCallback(
		(event: ReactPointerEvent<HTMLDivElement>) => {
			if (!gesturesEnabled) return;
			pointers.set(event.pointerId, {
				x: event.clientX,
				y: event.clientY,
			});
			event.currentTarget.setPointerCapture(event.pointerId);

			const activePointers = Array.from(pointers.values());
			if (activePointers.length === 1) {
				dragStartRef.current = {
					imageOffset,
					pointer: activePointers[0],
				};
				pinchStartRef.current = null;
				return;
			}

			if (activePointers.length === 2) {
				dragStartRef.current = null;
				pinchStartRef.current = {
					center: midpoint(activePointers[0], activePointers[1]),
					distance: distanceBetween(activePointers[0], activePointers[1]),
					imageOffset,
					zoom,
				};
			}
		},
		[gesturesEnabled, imageOffset, pointers, zoom],
	);

	const handlePointerMove = useCallback(
		(event: ReactPointerEvent<HTMLDivElement>) => {
			if (!gesturesEnabled) return;
			if (!pointers.has(event.pointerId)) return;
			pointers.set(event.pointerId, {
				x: event.clientX,
				y: event.clientY,
			});

			const activePointers = Array.from(pointers.values());
			if (activePointers.length === 2 && pinchStartRef.current) {
				event.preventDefault();
				const currentDistance = distanceBetween(
					activePointers[0],
					activePointers[1],
				);
				const currentCenter = midpoint(activePointers[0], activePointers[1]);
				if (pinchStartRef.current.distance <= 0) return;
				const nextZoom =
					pinchStartRef.current.zoom *
					(currentDistance / pinchStartRef.current.distance);
				const clampedZoom = clamp(nextZoom, MIN_ZOOM, MAX_ZOOM);
				const nextOffset = {
					x:
						pinchStartRef.current.imageOffset.x +
						currentCenter.x -
						pinchStartRef.current.center.x,
					y:
						pinchStartRef.current.imageOffset.y +
						currentCenter.y -
						pinchStartRef.current.center.y,
				};
				setZoom(clampedZoom);
				setImageOffset(clampOffset(nextOffset, clampedZoom));
				return;
			}

			if (activePointers.length === 1 && dragStartRef.current && zoom > 1) {
				event.preventDefault();
				const nextOffset = {
					x:
						dragStartRef.current.imageOffset.x +
						activePointers[0].x -
						dragStartRef.current.pointer.x,
					y:
						dragStartRef.current.imageOffset.y +
						activePointers[0].y -
						dragStartRef.current.pointer.y,
				};
				setImageOffset(clampOffset(nextOffset, zoom));
			}
		},
		[clampOffset, gesturesEnabled, pointers, zoom],
	);

	const handlePointerEnd = useCallback(
		(event: ReactPointerEvent<HTMLDivElement>) => {
			if (!gesturesEnabled) return;
			pointers.delete(event.pointerId);
			if (event.currentTarget.hasPointerCapture(event.pointerId)) {
				event.currentTarget.releasePointerCapture(event.pointerId);
			}

			const activePointers = Array.from(pointers.values());
			if (activePointers.length === 1) {
				dragStartRef.current = {
					imageOffset: clampOffset(imageOffset, zoom),
					pointer: activePointers[0],
				};
				pinchStartRef.current = null;
				return;
			}

			dragStartRef.current = null;
			pinchStartRef.current = null;
			setImageOffset((current) => clampOffset(current, zoom));
		},
		[clampOffset, gesturesEnabled, imageOffset, pointers, zoom],
	);

	useEffect(() => {
		if (gesturesEnabled) return;
		pointersRef.current?.clear();
		dragStartRef.current = null;
		pinchStartRef.current = null;
	}, [gesturesEnabled]);

	useEffect(() => {
		const handleResize = () => {
			setImageOffset((current) => clampOffset(current, zoom));
		};
		window.addEventListener("resize", handleResize);
		return () => window.removeEventListener("resize", handleResize);
	}, [clampOffset, zoom]);

	useEffect(() => {
		if (!gesturesEnabled) return;
		const wheelTarget = gestureSurfaceRef?.current ?? viewportRef.current;
		if (!wheelTarget) return;

		const handleWheel = (event: WheelEvent) => {
			if (!event.ctrlKey && !event.metaKey) return;
			event.preventDefault();
			const direction = event.deltaY > 0 ? -1 : 1;
			setClampedZoom(zoom + direction * ZOOM_STEP, {
				x: event.clientX,
				y: event.clientY,
			});
		};

		wheelTarget.addEventListener("wheel", handleWheel, { passive: false });
		return () => {
			wheelTarget.removeEventListener("wheel", handleWheel);
		};
	}, [gestureSurfaceRef, gesturesEnabled, setClampedZoom, viewportRef, zoom]);

	return {
		canZoomIn,
		canZoomOut,
		handlePointerDown,
		handlePointerEnd,
		handlePointerMove,
		imageStyle,
		resetImageTransform,
		rotateRight,
		zoom,
		zoomIn,
		zoomOut,
		zoomPercent,
	};
}
