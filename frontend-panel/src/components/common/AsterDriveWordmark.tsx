import type { ComponentProps } from "react";
import { cn } from "@/lib/utils";
import { useFrontendConfigStore } from "@/stores/frontendConfigStore";
import { useThemeStore } from "@/stores/themeStore";

type SurfaceTheme = "light" | "dark";

type AsterDriveWordmarkProps = Omit<ComponentProps<"img">, "src"> & {
	surfaceTheme?: SurfaceTheme;
};

export function AsterDriveWordmark({
	alt,
	className,
	draggable = false,
	surfaceTheme,
	...props
}: AsterDriveWordmarkProps) {
	const wordmarkDarkUrl = useFrontendConfigStore(
		(s) => s.branding.wordmarkDarkUrl,
	);
	const wordmarkLightUrl = useFrontendConfigStore(
		(s) => s.branding.wordmarkLightUrl,
	);
	const resolvedTheme = useThemeStore((s) => s.resolvedTheme);
	const effectiveTheme = surfaceTheme ?? resolvedTheme;

	return (
		<img
			src={effectiveTheme === "dark" ? wordmarkLightUrl : wordmarkDarkUrl}
			alt={alt}
			draggable={draggable}
			className={cn("block h-auto w-auto", className)}
			{...props}
		/>
	);
}
