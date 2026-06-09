import type { ReactNode } from "react";
import { useTranslation } from "react-i18next";
import { Icon } from "@/components/ui/icon";
import { Tabs, TabsList, TabsTrigger } from "@/components/ui/tabs";
import { SECURITY_PANES, type SecurityPane } from "./securityPanes";

interface SecurityTabsShellProps {
	activePane: SecurityPane;
	children: ReactNode;
	onActivePaneChange: (pane: SecurityPane) => void;
}

export function SecurityTabsShell({
	activePane,
	children,
	onActivePaneChange,
}: SecurityTabsShellProps) {
	const { t } = useTranslation(["settings"]);
	const activePaneSummary =
		SECURITY_PANES.find((pane) => pane.value === activePane) ??
		SECURITY_PANES[0];
	const activePaneIndex = SECURITY_PANES.findIndex(
		(pane) => pane.value === activePane,
	);

	return (
		<Tabs
			value={activePane}
			onValueChange={(value) => onActivePaneChange(value as SecurityPane)}
			className="gap-4 lg:grid lg:grid-cols-[240px_minmax(0,1fr)] lg:items-start"
		>
			<div className="space-y-2">
				<div className="flex items-center justify-between gap-3 rounded-lg bg-muted/30 px-3 py-2 lg:hidden">
					<div className="flex min-w-0 items-center gap-2">
						<div className="flex size-8 shrink-0 items-center justify-center rounded-md bg-background text-foreground shadow-xs">
							<Icon name={activePaneSummary.icon} className="size-4" />
						</div>
						<div className="min-w-0">
							<p className="truncate text-sm font-medium">
								{t(activePaneSummary.labelKey)}
							</p>
							<p className="truncate text-xs text-muted-foreground">
								{t(activePaneSummary.descriptionKey)}
							</p>
						</div>
					</div>
					<span className="shrink-0 text-xs tabular-nums text-muted-foreground">
						{activePaneIndex + 1}/{SECURITY_PANES.length}
					</span>
				</div>

				<TabsList className="grid !h-auto min-h-11 w-full grid-cols-5 gap-1 rounded-lg p-1 sm:!h-11 lg:hidden">
					{SECURITY_PANES.map((pane) => {
						const label = t(pane.labelKey);
						return (
							<TabsTrigger
								key={pane.value}
								value={pane.value}
								aria-label={label}
								title={label}
								className="h-10 min-w-0 px-0 py-0 sm:h-full sm:px-3"
							>
								<Icon name={pane.icon} className="size-4" />
								<span className="hidden truncate sm:inline">{label}</span>
							</TabsTrigger>
						);
					})}
				</TabsList>

				<TabsList className="hidden !h-auto w-full rounded-none bg-transparent p-0 shadow-none lg:grid lg:gap-1">
					{SECURITY_PANES.map((pane) => {
						const label = t(pane.labelKey);
						return (
							<TabsTrigger
								key={pane.value}
								value={pane.value}
								aria-label={label}
								title={label}
								className="h-auto justify-start rounded-lg border px-3 py-3 text-left data-active:border-primary/20 data-active:bg-primary/10 data-active:shadow-none"
							>
								<Icon name={pane.icon} className="mt-0.5 size-4 shrink-0" />
								<span className="min-w-0">
									<span className="block truncate text-sm font-medium">
										{label}
									</span>
									<span className="mt-0.5 block truncate text-xs font-normal text-muted-foreground">
										{t(pane.descriptionKey)}
									</span>
								</span>
							</TabsTrigger>
						);
					})}
				</TabsList>
			</div>

			<div className="min-w-0">{children}</div>
		</Tabs>
	);
}
