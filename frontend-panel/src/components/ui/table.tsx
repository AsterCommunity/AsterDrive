import type * as React from "react";
import { use } from "react";

import { ScrollAreaContext } from "@/components/ui/scrollAreaContext";
import { cn } from "@/lib/utils";

function Table({
	className,
	frameless = false,
	...props
}: React.ComponentProps<"table"> & {
	/** D9 用户页去框化：不渲染带边框的容器，表格直接坐页面背景 */
	frameless?: boolean;
}) {
	const isInsideScrollArea = use(ScrollAreaContext);
	const table = (
		<table
			data-slot="table"
			className={cn("w-full caption-bottom text-sm", className)}
			{...props}
		/>
	);

	if (isInsideScrollArea) {
		return table;
	}

	if (frameless) {
		return <div className="relative w-full overflow-x-auto">{table}</div>;
	}

	return (
		<div
			data-slot="table-container"
			data-theme-surface="panel"
			className="relative w-full overflow-x-auto rounded-xl border border-border/70 bg-card shadow-xs dark:shadow-none"
		>
			{table}
		</div>
	);
}

function TableHeader({ className, ...props }: React.ComponentProps<"thead">) {
	return (
		<thead
			data-slot="table-header"
			className={cn("[&_tr]:border-b [&_tr]:border-border/60", className)}
			{...props}
		/>
	);
}

function TableBody({ className, ...props }: React.ComponentProps<"tbody">) {
	return (
		<tbody
			data-slot="table-body"
			className={cn("[&_tr:last-child]:border-0", className)}
			{...props}
		/>
	);
}

function TableFooter({ className, ...props }: React.ComponentProps<"tfoot">) {
	return (
		<tfoot
			data-slot="table-footer"
			data-theme-surface="panel"
			className={cn(
				"border-t border-border/60 bg-muted/35 font-medium [&>tr]:last:border-b-0",
				className,
			)}
			{...props}
		/>
	);
}

function TableRow({ className, ...props }: React.ComponentProps<"tr">) {
	return (
		<tr
			data-slot="table-row"
			data-theme-surface="panel"
			className={cn(
				"border-b border-border/55 transition-colors hover:bg-accent/35 data-[state=selected]:bg-accent/60",
				className,
			)}
			{...props}
		/>
	);
}

function TableHead({ className, ...props }: React.ComponentProps<"th">) {
	return (
		<th
			data-slot="table-head"
			data-theme-surface="panel"
			className={cn(
				"sticky top-0 z-10 h-10 bg-card/95 px-2 text-left align-middle font-medium whitespace-nowrap text-muted-foreground backdrop-blur first:pl-4 last:pr-4 md:first:pl-6 md:last:pr-6 [&:has([role=checkbox])]:pr-0",
				className,
			)}
			{...props}
		/>
	);
}

function TableCell({ className, ...props }: React.ComponentProps<"td">) {
	return (
		<td
			data-slot="table-cell"
			className={cn(
				"p-2 align-middle whitespace-nowrap first:pl-4 last:pr-4 md:first:pl-6 md:last:pr-6 [&:has([role=checkbox])]:pr-0",
				className,
			)}
			{...props}
		/>
	);
}

function TableCaption({
	className,
	...props
}: React.ComponentProps<"caption">) {
	return (
		<caption
			data-slot="table-caption"
			className={cn("mt-4 text-sm text-muted-foreground", className)}
			{...props}
		/>
	);
}

export {
	Table,
	TableBody,
	TableCaption,
	TableCell,
	TableFooter,
	TableHead,
	TableHeader,
	TableRow,
};
