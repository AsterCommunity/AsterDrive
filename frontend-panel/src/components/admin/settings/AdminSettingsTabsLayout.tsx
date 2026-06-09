import type { MutableRefObject, ReactNode, RefObject } from "react";
import { Button } from "@/components/ui/button";
import { buttonVariants } from "@/components/ui/buttonVariants";
import {
	DropdownMenu,
	DropdownMenuContent,
	DropdownMenuRadioGroup,
	DropdownMenuRadioItem,
	DropdownMenuTrigger,
} from "@/components/ui/dropdown-menu";
import { Icon, type IconName } from "@/components/ui/icon";
import { Tabs, TabsList, TabsTrigger } from "@/components/ui/tabs";

const COMPACT_NAV_TAB_TRIGGER_CLASS =
	"h-10 flex-none rounded-none px-0 text-sm font-medium";
const COMPACT_NAV_TAB_CONTENT_CLASS =
	"inline-flex items-center gap-2 px-2.5 py-2";
const COMPACT_NAV_OVERFLOW_TRIGGER_CLASS = buttonVariants({
	variant: "secondary",
	size: "sm",
	className: "shrink-0 rounded-lg px-3",
});

type TranslationFn = (key: string, options?: Record<string, unknown>) => string;

export type AdminSettingsCategorySummary = {
	category: string;
	description?: string;
	icon: IconName;
	label: string;
};

interface AdminSettingsTabsLayoutProps {
	activeCategorySummary: AdminSettingsCategorySummary | null;
	activeTab: string;
	categorySummaries: AdminSettingsCategorySummary[];
	children: ReactNode;
	compactInlineSummaries: AdminSettingsCategorySummary[];
	compactNavContainerRef: RefObject<HTMLDivElement | null>;
	compactOrderedSummaries: AdminSettingsCategorySummary[];
	compactOverflowActiveSummary: AdminSettingsCategorySummary | null;
	compactOverflowDefaultMeasureRef: RefObject<HTMLButtonElement | null>;
	compactOverflowMeasureRefs: MutableRefObject<
		Record<string, HTMLButtonElement | null>
	>;
	compactOverflowSummaries: AdminSettingsCategorySummary[];
	compactTabMeasureRefs: MutableRefObject<
		Record<string, HTMLButtonElement | null>
	>;
	handleCategoryChange: (value: string) => void;
	isCompactNavigation: boolean;
	isDesktopNavigation: boolean;
	isMobileNavigation: boolean;
	t: TranslationFn;
}

function renderCategoryMenuItems({
	activeTab,
	handleCategoryChange,
	summaries,
}: {
	activeTab: string;
	handleCategoryChange: (value: string) => void;
	summaries: AdminSettingsCategorySummary[];
}) {
	return (
		<DropdownMenuRadioGroup
			value={activeTab}
			onValueChange={(value) => handleCategoryChange(value)}
		>
			{summaries.map((summary) => (
				<DropdownMenuRadioItem
					key={summary.category}
					value={summary.category}
					className="gap-2 pr-10"
				>
					<Icon name={summary.icon} className="size-4 text-muted-foreground" />
					<span className="min-w-0 flex-1 truncate">{summary.label}</span>
				</DropdownMenuRadioItem>
			))}
		</DropdownMenuRadioGroup>
	);
}

function CompactNavigationTabContent({
	className,
	summary,
}: {
	className?: string;
	summary: AdminSettingsCategorySummary;
}) {
	return (
		<span
			className={
				className
					? `${COMPACT_NAV_TAB_CONTENT_CLASS} ${className}`
					: COMPACT_NAV_TAB_CONTENT_CLASS
			}
		>
			<Icon name={summary.icon} className="size-4" />
			<span>{summary.label}</span>
		</span>
	);
}

function DesktopNavigation({
	categorySummaries,
}: {
	categorySummaries: AdminSettingsCategorySummary[];
}) {
	return (
		<div className="sticky top-2 self-start min-h-0 w-[15.5rem] flex-none border-r border-border/50 pr-3">
			<TabsList
				variant="line"
				className="h-auto w-full flex-col items-stretch gap-1 bg-transparent p-0"
			>
				{categorySummaries.map((summary) => (
					<TabsTrigger
						key={summary.category}
						value={summary.category}
						className="group h-auto w-full justify-start rounded-lg border-0 p-0 text-left whitespace-normal text-foreground/75 after:hidden hover:text-foreground data-active:bg-transparent data-active:text-foreground"
					>
						<div className="flex w-full items-start gap-2.5 rounded-lg px-3 py-2.5 transition-colors duration-150 group-hover:bg-muted/40 group-data-[active]:bg-muted/60">
							<div className="flex size-8 shrink-0 items-center justify-center rounded-md bg-muted/75 text-muted-foreground transition-colors duration-150 group-data-[active]:bg-background group-data-[active]:text-foreground">
								<Icon name={summary.icon} className="size-4" />
							</div>
							<div className="min-w-0 flex-1 space-y-1">
								<p className="break-words text-sm font-medium leading-5">
									{summary.label}
								</p>
								{summary.description ? (
									<p className="break-words text-xs leading-4 text-muted-foreground">
										{summary.description}
									</p>
								) : null}
							</div>
						</div>
					</TabsTrigger>
				))}
			</TabsList>
		</div>
	);
}

function CompactNavigation({
	activeTab,
	compactInlineSummaries,
	compactNavContainerRef,
	compactOverflowActiveSummary,
	compactOverflowSummaries,
	handleCategoryChange,
	t,
}: {
	activeTab: string;
	compactInlineSummaries: AdminSettingsCategorySummary[];
	compactNavContainerRef: RefObject<HTMLDivElement | null>;
	compactOverflowActiveSummary: AdminSettingsCategorySummary | null;
	compactOverflowSummaries: AdminSettingsCategorySummary[];
	handleCategoryChange: (value: string) => void;
	t: TranslationFn;
}) {
	return (
		<div
			ref={compactNavContainerRef}
			className="flex flex-none items-end gap-3"
		>
			<TabsList
				variant="line"
				className="h-auto min-w-0 flex-1 justify-start gap-2 overflow-hidden border-b border-border/40 bg-transparent px-0 pb-2"
			>
				{compactInlineSummaries.map((summary) => (
					<TabsTrigger
						key={summary.category}
						value={summary.category}
						className={`${COMPACT_NAV_TAB_TRIGGER_CLASS} text-muted-foreground hover:text-foreground data-active:text-foreground`}
					>
						<CompactNavigationTabContent summary={summary} />
					</TabsTrigger>
				))}
			</TabsList>

			{compactOverflowSummaries.length > 0 ? (
				<DropdownMenu>
					<DropdownMenuTrigger
						render={
							<Button
								variant={compactOverflowActiveSummary ? "secondary" : "ghost"}
								size="sm"
								className="shrink-0 rounded-lg px-3"
							>
								<Icon
									name={compactOverflowActiveSummary?.icon ?? "DotsThree"}
									className="size-4"
								/>
								<span>
									{compactOverflowActiveSummary?.label ?? t("core:more")}
								</span>
							</Button>
						}
					/>
					<DropdownMenuContent align="end" className="w-64 min-w-64">
						{renderCategoryMenuItems({
							activeTab,
							handleCategoryChange,
							summaries: compactOverflowSummaries,
						})}
					</DropdownMenuContent>
				</DropdownMenu>
			) : null}
		</div>
	);
}

function CompactNavigationMeasurements({
	compactOrderedSummaries,
	compactOverflowDefaultMeasureRef,
	compactOverflowMeasureRefs,
	compactTabMeasureRefs,
	isCompactNavigation,
	t,
}: {
	compactOrderedSummaries: AdminSettingsCategorySummary[];
	compactOverflowDefaultMeasureRef: RefObject<HTMLButtonElement | null>;
	compactOverflowMeasureRefs: MutableRefObject<
		Record<string, HTMLButtonElement | null>
	>;
	compactTabMeasureRefs: MutableRefObject<
		Record<string, HTMLButtonElement | null>
	>;
	isCompactNavigation: boolean;
	t: TranslationFn;
}) {
	if (!isCompactNavigation) {
		return null;
	}

	return (
		<div
			aria-hidden="true"
			className="pointer-events-none fixed top-0 left-0 -z-10 flex items-center gap-2 opacity-0"
		>
			{compactOrderedSummaries.map((summary) => (
				<button
					key={`measure-tab-${summary.category}`}
					ref={(node) => {
						compactTabMeasureRefs.current[summary.category] = node;
					}}
					type="button"
					className={`${COMPACT_NAV_TAB_TRIGGER_CLASS} inline-flex items-center whitespace-nowrap`}
				>
					<CompactNavigationTabContent summary={summary} />
				</button>
			))}
			<button
				ref={compactOverflowDefaultMeasureRef}
				type="button"
				className={COMPACT_NAV_OVERFLOW_TRIGGER_CLASS}
			>
				<Icon name="DotsThree" className="size-4" />
				<span>{t("core:more")}</span>
			</button>
			{compactOrderedSummaries.map((summary) => (
				<button
					key={`measure-overflow-${summary.category}`}
					ref={(node) => {
						compactOverflowMeasureRefs.current[summary.category] = node;
					}}
					type="button"
					className={COMPACT_NAV_OVERFLOW_TRIGGER_CLASS}
				>
					<Icon name={summary.icon} className="size-4" />
					<span>{summary.label}</span>
				</button>
			))}
		</div>
	);
}

function MobileNavigation({
	activeCategorySummary,
	activeTab,
	categorySummaries,
	handleCategoryChange,
	t,
}: {
	activeCategorySummary: AdminSettingsCategorySummary | null;
	activeTab: string;
	categorySummaries: AdminSettingsCategorySummary[];
	handleCategoryChange: (value: string) => void;
	t: TranslationFn;
}) {
	if (!activeCategorySummary) {
		return null;
	}

	return (
		<div className="flex-none space-y-2">
			<p className="text-xs font-medium tracking-[0.18em] text-muted-foreground uppercase">
				{t("config_category")}
			</p>
			<DropdownMenu>
				<DropdownMenuTrigger
					render={
						<Button
							variant="outline"
							className="h-auto w-full justify-between rounded-lg px-3 py-2.5"
						>
							<span className="flex min-w-0 items-center gap-2.5">
								<span className="flex size-8 shrink-0 items-center justify-center rounded-md bg-muted text-muted-foreground">
									<Icon name={activeCategorySummary.icon} className="size-4" />
								</span>
								<span className="min-w-0 text-left">
									<span className="block truncate text-sm font-medium">
										{activeCategorySummary.label}
									</span>
									<span className="block text-xs text-muted-foreground">
										{t("config_category")}
									</span>
								</span>
							</span>
							<span className="ml-3 flex shrink-0 items-center gap-2">
								<Icon
									name="CaretDown"
									className="size-4 text-muted-foreground"
								/>
							</span>
						</Button>
					}
				/>
				<DropdownMenuContent
					align="start"
					className="w-[min(24rem,calc(100vw-2rem))] min-w-64"
				>
					{renderCategoryMenuItems({
						activeTab,
						handleCategoryChange,
						summaries: categorySummaries,
					})}
				</DropdownMenuContent>
			</DropdownMenu>
		</div>
	);
}

export function AdminSettingsTabsLayout({
	activeCategorySummary,
	activeTab,
	categorySummaries,
	children,
	compactInlineSummaries,
	compactNavContainerRef,
	compactOrderedSummaries,
	compactOverflowActiveSummary,
	compactOverflowDefaultMeasureRef,
	compactOverflowMeasureRefs,
	compactOverflowSummaries,
	compactTabMeasureRefs,
	handleCategoryChange,
	isCompactNavigation,
	isDesktopNavigation,
	isMobileNavigation,
	t,
}: AdminSettingsTabsLayoutProps) {
	return (
		<Tabs
			orientation={isDesktopNavigation ? "vertical" : "horizontal"}
			value={activeTab}
			onValueChange={handleCategoryChange}
			className={
				isDesktopNavigation
					? "flex min-w-0 flex-row items-start gap-6"
					: "flex min-w-0 flex-col gap-5"
			}
		>
			{isMobileNavigation ? (
				<MobileNavigation
					activeCategorySummary={activeCategorySummary}
					activeTab={activeTab}
					categorySummaries={categorySummaries}
					handleCategoryChange={handleCategoryChange}
					t={t}
				/>
			) : null}
			{isCompactNavigation ? (
				<CompactNavigation
					activeTab={activeTab}
					compactInlineSummaries={compactInlineSummaries}
					compactNavContainerRef={compactNavContainerRef}
					compactOverflowActiveSummary={compactOverflowActiveSummary}
					compactOverflowSummaries={compactOverflowSummaries}
					handleCategoryChange={handleCategoryChange}
					t={t}
				/>
			) : null}
			{isDesktopNavigation ? (
				<DesktopNavigation categorySummaries={categorySummaries} />
			) : null}
			<CompactNavigationMeasurements
				compactOrderedSummaries={compactOrderedSummaries}
				compactOverflowDefaultMeasureRef={compactOverflowDefaultMeasureRef}
				compactOverflowMeasureRefs={compactOverflowMeasureRefs}
				compactTabMeasureRefs={compactTabMeasureRefs}
				isCompactNavigation={isCompactNavigation}
				t={t}
			/>

			<div
				className={isDesktopNavigation ? "min-w-0 flex-1 pr-2" : "min-w-0 pr-2"}
			>
				{children}
			</div>
		</Tabs>
	);
}
