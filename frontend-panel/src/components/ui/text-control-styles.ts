const textControlBaseClasses =
	"w-full min-w-0 rounded-lg border border-input/80 bg-card/70 text-base shadow-xs transition-[background-color,border-color,box-shadow] outline-none placeholder:text-muted-foreground md:text-sm dark:bg-input/25 dark:shadow-none";

const textControlStateClasses =
	"focus-visible:border-ring focus-visible:bg-background focus-visible:inset-ring-3 focus-visible:inset-ring-ring/30 disabled:pointer-events-none disabled:cursor-not-allowed disabled:bg-muted/60 disabled:opacity-60 aria-invalid:border-destructive aria-invalid:inset-ring-3 aria-invalid:inset-ring-destructive/20 aria-invalid:focus-visible:inset-ring-destructive/45 dark:focus-visible:bg-input/35 dark:disabled:bg-input/80 dark:aria-invalid:border-destructive/50 dark:aria-invalid:inset-ring-destructive/40 dark:aria-invalid:focus-visible:inset-ring-destructive/60 forced-colors:focus-visible:-outline-offset-2 forced-colors:focus-visible:outline-2";

export { textControlBaseClasses, textControlStateClasses };
