export const ADMIN_SETTINGS_CONTENT_MAX_WIDTH_CLASS = "max-w-4xl";

export const ADMIN_SETTINGS_PANEL_ANIMATION_BASE_CLASS =
	"animate-in fade-in duration-150 ease-out motion-reduce:animate-none";

export const ADMIN_SETTINGS_PANEL_ANIMATION_BY_DIRECTION = {
	backward: `${ADMIN_SETTINGS_PANEL_ANIMATION_BASE_CLASS} slide-in-from-left-4`,
	forward: `${ADMIN_SETTINGS_PANEL_ANIMATION_BASE_CLASS} slide-in-from-right-4`,
} as const;

export const ADMIN_SETTINGS_PADDING_TRANSITION_CLASS =
	"transition-[padding-bottom] duration-[150ms] ease-out";

export const ADMIN_SETTINGS_SAVE_BAR_ENTER_CLASS =
	"pointer-events-auto animate-in fade-in slide-in-from-bottom-2 duration-[150ms] ease-out";

export const ADMIN_SETTINGS_SAVE_BAR_EXIT_CLASS =
	"pointer-events-none animate-out fade-out slide-out-to-bottom-2 duration-[120ms] ease-in";
