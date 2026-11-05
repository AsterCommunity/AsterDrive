import { useState } from "react";
import { useTranslation } from "react-i18next";
import { Button } from "@/components/ui/button";
import {
	Combobox,
	ComboboxContent,
	ComboboxInput,
	ComboboxItem,
	ComboboxList,
	ComboboxTrigger,
	ComboboxValue,
} from "@/components/ui/combobox";
import { Icon } from "@/components/ui/icon";
import { useAdminLookup } from "@/hooks/useAdminLookup";
import { ADMIN_CONTROL_HEIGHT_CLASS } from "@/lib/constants";
import type {
	AdminLookupKind,
	AdminLookupOption,
} from "@/services/adminLookupService";

export function AdminSearchFilter({
	kind,
	value,
	onChange,
	label,
}: {
	kind: AdminLookupKind;
	value: number | undefined;
	onChange: (value: number | undefined) => void;
	label: string;
}) {
	const { t } = useTranslation("admin");
	const [open, setOpen] = useState(false);
	const lookup = useAdminLookup(kind, value, open);
	const searchPlaceholder = t(
		kind === "policies"
			? "filter_search_policy_placeholder"
			: "filter_search_placeholder",
	);
	// Retain an unavailable selection without pretending its ID is its name.
	const selected: AdminLookupOption | null =
		lookup.selected ??
		(value == null
			? null
			: {
					id: value,
					label: t(
						lookup.selectedLoading
							? "core:loading"
							: "filter_selection_unavailable",
					),
					description: `#${value}`,
				});
	return (
		<Combobox
			items={lookup.options}
			value={selected}
			open={open}
			onOpenChange={(next) => {
				setOpen(next);
				if (!next) lookup.setSearch("");
			}}
			onValueChange={(option) => {
				lookup.rememberSelection(option);
				onChange(option?.id);
				setOpen(false);
				lookup.setSearch("");
			}}
			itemToStringLabel={(option) => option.label}
			isItemEqualToValue={(item, selected) => item.id === selected.id}
			filter={null}
			inputValue={lookup.search}
			onInputValueChange={(next, details) => {
				if (
					details.reason === "input-change" ||
					details.reason === "input-clear"
				)
					lookup.setSearch(next);
			}}
		>
			<ComboboxTrigger
				render={
					<Button
						variant="outline"
						className={`${ADMIN_CONTROL_HEIGHT_CLASS} min-w-44 justify-between font-normal`}
					/>
				}
				aria-label={label}
			>
				<span className="max-w-52 truncate">
					<ComboboxValue placeholder={label} />
				</span>
			</ComboboxTrigger>
			<ComboboxContent className="min-w-72">
				<ComboboxInput
					showTrigger={false}
					placeholder={searchPlaceholder}
					aria-label={searchPlaceholder}
				/>
				<div className="flex items-center justify-between px-2 py-1">
					<span className="text-xs text-muted-foreground">{label}</span>
					<Button
						variant="ghost"
						size="sm"
						onClick={() => {
							onChange(undefined);
							setOpen(false);
							lookup.setSearch("");
						}}
					>
						{t("filter_all")}
					</Button>
				</div>
				<ComboboxList>
					{(option: AdminLookupOption) => (
						<ComboboxItem key={option.id} value={option} className="min-h-11">
							<div className="min-w-0">
								<div className="truncate">{option.label}</div>
								<div className="truncate text-xs text-muted-foreground">
									{option.description}
								</div>
							</div>
						</ComboboxItem>
					)}
				</ComboboxList>
				{lookup.loading ? (
					<p className="p-3 text-sm text-muted-foreground" role="status">
						{t("core:loading")}
					</p>
				) : lookup.failed ? (
					<Button variant="ghost" className="w-full" onClick={lookup.retry}>
						<Icon name="ArrowClockwise" />
						{t("filter_retry")}
					</Button>
				) : lookup.options.length === 0 ? (
					<p className="p-3 text-sm text-muted-foreground">
						{t("filter_no_results")}
					</p>
				) : null}
				{lookup.hasMore && !lookup.loading ? (
					<Button variant="ghost" className="w-full" onClick={lookup.loadMore}>
						{t("filter_load_more")}
					</Button>
				) : null}
			</ComboboxContent>
		</Combobox>
	);
}
