import type { ComponentProps, ReactNode } from "react";
import { ScrollArea } from "@/components/ui/scroll-area";

interface WorkspaceSidebarLayoutProps {
	header: ReactNode;
	tree: ReactNode;
	navigation: ReactNode;
	footer: ReactNode;
	scrollProps?: { "data-testid"?: string } & Omit<
		ComponentProps<typeof ScrollArea>,
		"children" | "className"
	>;
}

export function WorkspaceSidebarLayout({
	header,
	tree,
	navigation,
	footer,
	scrollProps,
}: WorkspaceSidebarLayoutProps) {
	return (
		<div className="flex h-full min-h-0 flex-col overflow-hidden overscroll-contain">
			<div className="shrink-0 px-3 py-2 sm:py-2.5">{header}</div>
			<ScrollArea {...scrollProps} className="min-h-0 flex-1">
				<div className="flex min-h-full flex-col">
					{tree}
					<div className="mt-auto space-y-5 pt-4">{navigation}</div>
				</div>
			</ScrollArea>
			{footer}
		</div>
	);
}
