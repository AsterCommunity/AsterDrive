// Shared by text controls, including the transparent code-editor input.
export const textControlFocusClasses =
	"focus-visible:border-ring focus-visible:ring-3 focus-visible:ring-ring/30 aria-invalid:border-destructive aria-invalid:ring-3 aria-invalid:ring-destructive/20 aria-invalid:focus-visible:border-destructive aria-invalid:focus-visible:ring-destructive/50 dark:aria-invalid:border-destructive/50 dark:aria-invalid:ring-destructive/40 dark:aria-invalid:focus-visible:ring-destructive/70 forced-colors:focus-visible:outline-solid forced-colors:focus-visible:outline-2 forced-colors:focus-visible:outline-offset-0";

export const textControlClasses =
	"w-full min-w-0 rounded-lg border border-input/80 bg-card/70 text-base shadow-xs transition-[background-color,border-color,box-shadow] outline-none placeholder:text-muted-foreground focus-visible:bg-background disabled:pointer-events-none disabled:cursor-not-allowed disabled:bg-muted/60 disabled:opacity-60 md:text-sm dark:bg-input/25 dark:shadow-none dark:focus-visible:bg-input/35 dark:disabled:bg-input/80";
