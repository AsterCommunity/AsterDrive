export const MFA_MOTION_DURATION_MS = 240;
export const MFA_MOTION_EASING = "cubic-bezier(0.22, 1, 0.36, 1)";
export const MFA_STEP_SWAP_DELAY_MS = 120;

export function shouldReduceMotion() {
	return (
		typeof window !== "undefined" &&
		typeof window.matchMedia === "function" &&
		window.matchMedia("(prefers-reduced-motion: reduce)").matches
	);
}
