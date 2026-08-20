import { useTranslation } from "react-i18next";
import { Icon } from "@/components/ui/icon";
import { Input } from "@/components/ui/input";
import {
	Tooltip,
	TooltipContent,
	TooltipTrigger,
} from "@/components/ui/tooltip";
import { cn } from "@/lib/utils";
import {
	COLOR_PRESETS,
	isColorPreset,
	useThemeStore,
} from "@/stores/themeStore";

const presets = [
	{ id: "blue", labelKey: "color_blue", color: COLOR_PRESETS.blue },
	{ id: "green", labelKey: "color_green", color: COLOR_PRESETS.green },
	{ id: "purple", labelKey: "color_purple", color: COLOR_PRESETS.purple },
	{ id: "orange", labelKey: "color_orange", color: COLOR_PRESETS.orange },
] as const;

export function ColorPresetPicker() {
	const { t } = useTranslation("core");
	const { colorPreset, setColorPreset } = useThemeStore();
	const hasPresetSelection = presets.some((p) => p.color === colorPreset);

	return (
		<div className="flex flex-wrap items-center gap-3">
			<div className="flex gap-2">
				{presets.map((p) => {
					const selected = colorPreset === p.color;
					const label = t(p.labelKey);
					return (
						<Tooltip key={p.id}>
							<TooltipTrigger
								render={
									<button
										type="button"
										aria-label={label}
										aria-pressed={selected}
										onClick={() => setColorPreset(p.color)}
										className={cn(
											"flex size-7 items-center justify-center rounded-full border border-black/10 transition-transform focus-visible:outline-none focus-visible:ring-3 focus-visible:ring-ring/35",
											selected &&
												"scale-110 ring-2 ring-foreground ring-offset-2 ring-offset-background",
										)}
										style={{ backgroundColor: p.color }}
									/>
								}
							>
								{selected && (
									<Icon name="Check" className="size-3.5 text-white" />
								)}
							</TooltipTrigger>
							<TooltipContent>{label}</TooltipContent>
						</Tooltip>
					);
				})}
			</div>
			<Tooltip>
				<TooltipTrigger
					render={
						<div
							className={cn(
								"relative h-8 w-12 shrink-0 overflow-hidden rounded-full border border-border bg-card p-1 shadow-xs transition-transform focus-within:ring-3 focus-within:ring-ring/35 hover:scale-105",
								!hasPresetSelection &&
									"ring-2 ring-foreground ring-offset-2 ring-offset-background",
							)}
						/>
					}
				>
					<div
						className="h-full w-full rounded-full"
						style={{ backgroundColor: colorPreset }}
					/>
					<span className="sr-only">{t("custom_color")}</span>
					<Input
						type="color"
						value={colorPreset}
						onChange={(event) => {
							const nextColor = event.currentTarget.value;
							if (isColorPreset(nextColor)) {
								setColorPreset(nextColor);
							}
						}}
						className="absolute inset-0 h-full w-full cursor-pointer opacity-0"
						aria-label={t("custom_color")}
					/>
				</TooltipTrigger>
				<TooltipContent>{t("custom_color")}</TooltipContent>
			</Tooltip>
		</div>
	);
}
