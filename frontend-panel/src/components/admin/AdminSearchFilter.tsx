import { useEffect, useRef, useState } from "react";
import { useTranslation } from "react-i18next";
import { Button } from "@/components/ui/button";
import { Input } from "@/components/ui/input";
import {
	Popover,
	PopoverContent,
	PopoverTrigger,
} from "@/components/ui/popover";
import { adminPolicyService, adminUserService } from "@/services/adminService";

type Option = { id: number; label: string };
type FilterKind = "user" | "policy";

async function loadOptions(
	kind: FilterKind,
	keyword: string,
): Promise<Option[]> {
	if (kind === "user") {
		const page = await adminUserService.list({
			keyword: keyword || undefined,
			limit: 20,
		});
		return page.items.map((user) => ({
			id: user.id,
			label: `${user.username} (${user.email})`,
		}));
	}
	const page = await adminPolicyService.list({
		keyword: keyword || undefined,
		limit: 20,
	});
	return page.items.map((policy) => ({ id: policy.id, label: policy.name }));
}

async function loadSelected(kind: FilterKind, id: number): Promise<Option> {
	if (kind === "user") {
		const user = await adminUserService.get(id);
		return { id, label: `${user.username} (${user.email})` };
	}
	const policy = await adminPolicyService.get(id);
	return { id, label: policy.name };
}

export function AdminSearchFilter({
	kind,
	value,
	onChange,
}: {
	kind: FilterKind;
	value?: number;
	onChange: (value?: number) => void;
}) {
	const { t } = useTranslation("admin");
	const [open, setOpen] = useState(false);
	const [keyword, setKeyword] = useState("");
	const [options, setOptions] = useState<Option[]>([]);
	const [selected, setSelected] = useState<Option | null>(null);
	const selectedRef = useRef(selected);
	selectedRef.current = selected;
	const [loading, setLoading] = useState(false);
	const label =
		kind === "user" ? t("admin_filter_user") : t("admin_filter_policy");

	useEffect(() => {
		if (value == null) return;
		if (selectedRef.current?.id === value) return;
		let active = true;
		loadSelected(kind, value)
			.then((option) => {
				if (active) setSelected(option);
			})
			.catch(() => {
				if (active) setSelected(null);
			});
		return () => {
			active = false;
		};
	}, [kind, value]);

	useEffect(() => {
		if (!open) return;
		let active = true;
		const timer = window.setTimeout(() => {
			setLoading(true);
			loadOptions(kind, keyword.trim())
				.then((items) => {
					if (active) setOptions(items);
				})
				.catch(() => {
					if (active) setOptions([]);
				})
				.finally(() => {
					if (active) setLoading(false);
				});
		}, 300);
		return () => {
			active = false;
			window.clearTimeout(timer);
		};
	}, [kind, keyword, open]);

	return (
		<Popover open={open} onOpenChange={setOpen}>
			<PopoverTrigger
				render={
					<Button
						type="button"
						variant="outline"
						className="min-w-40 max-w-60 justify-between truncate"
					/>
				}
			>
				{selected && selected.id === value
					? selected.label
					: value == null
						? label
						: `#${value}`}
			</PopoverTrigger>
			<PopoverContent className="w-72">
				<Input
					aria-label={label}
					value={keyword}
					onChange={(event) => setKeyword(event.target.value)}
					placeholder={label}
				/>
				<div className="max-h-56 overflow-y-auto">
					{value != null && (
						<Button
							type="button"
							variant="ghost"
							className="w-full justify-start"
							onClick={() => {
								onChange(undefined);
								setOpen(false);
							}}
						>
							{t("admin_filter_all")}
						</Button>
					)}
					{loading ? (
						<p className="p-2 text-sm text-muted-foreground">
							{t("core:loading")}
						</p>
					) : (
						options.map((option) => (
							<Button
								key={option.id}
								type="button"
								variant="ghost"
								className="w-full justify-start truncate"
								onClick={() => {
									setSelected(option);
									onChange(option.id);
									setOpen(false);
								}}
							>
								{option.label}
							</Button>
						))
					)}
				</div>
			</PopoverContent>
		</Popover>
	);
}
