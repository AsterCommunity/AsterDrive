import { type ClassValue, clsx } from "clsx";
import { twMerge } from "tailwind-merge";

export function cn(...inputs: ClassValue[]) {
	return twMerge(clsx(inputs));
}

export function sidebarNavItemClass(isActive: boolean, extra?: ClassValue) {
	return cn(
		"flex select-none items-center gap-2 rounded-lg px-3 py-2 text-sm transition-[background-color,color,box-shadow]",
		isActive
			? "bg-accent text-accent-foreground font-medium"
			: "text-muted-foreground hover:bg-accent/45 hover:text-foreground",
		extra,
	);
}

export function folderTreeRowClass(isActive: boolean, extra?: ClassValue) {
	return cn(
		"flex w-full items-center gap-1 rounded-lg px-2 py-1.5 text-left text-sm transition-[background-color,color,box-shadow] hover:bg-accent/50 focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring/40 focus-visible:ring-offset-2",
		isActive
			? "bg-accent text-accent-foreground font-medium"
			: "text-foreground",
		extra,
	);
}
