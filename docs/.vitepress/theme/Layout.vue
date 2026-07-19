<script setup lang="ts">
import { useData } from "vitepress"
import DefaultTheme from "vitepress/theme"
import { nextTick, onBeforeUnmount, onMounted, provide, ref } from "vue"

const { isDark } = useData()
const { Layout } = DefaultTheme
const zoomedDiagram = ref("")
let cleanupAppearanceTransition: (() => void) | null = null

function canUseAppearanceTransition() {
	return (
		typeof document !== "undefined" &&
		typeof window !== "undefined" &&
		window.matchMedia("(prefers-reduced-motion: no-preference)").matches
	)
}

function getThemeSurfaceColor() {
	const style = getComputedStyle(document.documentElement)
	return (
		style.getPropertyValue("--vp-c-bg").trim() ||
		style.backgroundColor ||
		(isDark.value ? "#07100f" : "#f7fbfa")
	)
}

function createAppearanceTransitionLayer(background: string, targetIsDark: boolean) {
	const layer = document.createElement("div")
	layer.className = `aster-appearance-transition-layer ${targetIsDark ? "is-to-dark" : "is-to-light"}`
	layer.style.background = background
	document.body.appendChild(layer)
	return layer
}

function cleanupActiveAppearanceTransition() {
	cleanupAppearanceTransition?.()
	cleanupAppearanceTransition = null
}

async function animateLayer(layer: HTMLElement, targetIsDark: boolean) {
	const endBrightness = targetIsDark ? "brightness(0.82)" : "brightness(1.12)"
	const animation = layer.animate(
		[
			{
				opacity: 1,
				filter: "blur(0px) brightness(1)",
				transform: "scale(1)",
			},
			{
				opacity: 0.78,
				filter: `${endBrightness} blur(2px)`,
				transform: "scale(1.004)",
				offset: 0.36,
			},
			{
				opacity: 0,
				filter: `${endBrightness} blur(10px)`,
				transform: "scale(1.018)",
			},
		],
		{
			duration: 340,
			easing: "cubic-bezier(0.22, 1, 0.36, 1)",
			fill: "both",
		},
	)

	await animation.finished.catch(() => undefined)
}

provide("toggle-appearance", async () => {
	if (!canUseAppearanceTransition()) {
		isDark.value = !isDark.value
		return
	}

	cleanupActiveAppearanceTransition()

	const targetIsDark = !isDark.value
	const oldSurfaceColor = getThemeSurfaceColor()
	const layers: HTMLElement[] = []
	const layer = createAppearanceTransitionLayer(oldSurfaceColor, targetIsDark)
	layers.push(layer)

	cleanupAppearanceTransition = () => {
		for (const layer of layers) {
			layer.remove()
		}
	}

	isDark.value = targetIsDark
	await nextTick()
	await animateLayer(layer, targetIsDark)

	cleanupActiveAppearanceTransition()
})

function closeDiagramZoom() {
	zoomedDiagram.value = ""
	document.body.classList.remove("aster-diagram-zoom-open")
}

function openDiagramZoom(event: MouseEvent) {
	const target = event.target instanceof Element ? event.target : null
	const diagram = target?.closest(".vp-doc .mermaid")

	if (!(diagram instanceof HTMLElement)) {
		return
	}

	const svg = diagram.querySelector("svg")
	if (!svg) {
		return
	}

	zoomedDiagram.value = svg.outerHTML
	document.body.classList.add("aster-diagram-zoom-open")
}

function closeDiagramZoomOnEscape(event: KeyboardEvent) {
	if (event.key === "Escape") {
		closeDiagramZoom()
	}
}

onMounted(() => {
	document.addEventListener("click", openDiagramZoom)
	document.addEventListener("keydown", closeDiagramZoomOnEscape)
})

onBeforeUnmount(() => {
	document.removeEventListener("click", openDiagramZoom)
	document.removeEventListener("keydown", closeDiagramZoomOnEscape)
	document.body.classList.remove("aster-diagram-zoom-open")
	cleanupActiveAppearanceTransition()
})
</script>

<template>
	<Layout />
	<Teleport to="body">
		<div v-if="zoomedDiagram" class="aster-diagram-zoom" role="dialog" aria-modal="true" @click.self="closeDiagramZoom">
			<button class="aster-diagram-zoom-close" type="button" aria-label="Close diagram zoom" @click="closeDiagramZoom">
				×
			</button>
			<div class="aster-diagram-zoom-canvas" v-html="zoomedDiagram" />
		</div>
	</Teleport>
</template>

<style>
.aster-appearance-transition-layer {
	position: fixed;
	inset: 0;
	z-index: 10001;
	overflow: hidden;
	pointer-events: none;
	contain: paint;
	transform-origin: center;
	will-change: opacity, filter, transform;
}

.aster-appearance-transition-layer::after {
	position: absolute;
	inset: -18%;
	background:
		linear-gradient(
			115deg,
			transparent 0%,
			transparent 42%,
			rgb(255 255 255 / 0.2) 49%,
			transparent 58%,
			transparent 100%
		);
	content: "";
	opacity: 0.8;
	transform: translateX(-18%);
	animation: aster-appearance-sheen 340ms cubic-bezier(0.22, 1, 0.36, 1) both;
}

.aster-appearance-transition-layer.is-to-light::after {
	background:
		linear-gradient(
			115deg,
			transparent 0%,
			transparent 41%,
			rgb(255 255 255 / 0.32) 50%,
			transparent 60%,
			transparent 100%
		);
}

.aster-appearance-transition-layer.is-to-dark::after {
	background:
		linear-gradient(
			115deg,
			transparent 0%,
			transparent 40%,
			rgb(45 212 191 / 0.18) 49%,
			rgb(0 0 0 / 0.22) 57%,
			transparent 100%
		);
}

@keyframes aster-appearance-sheen {
	from {
		transform: translateX(-18%);
	}

	to {
		transform: translateX(18%);
	}
}

.VPSwitchAppearance {
	width: 22px !important;
}

.VPSwitchAppearance .check {
	transform: none !important;
}
</style>
