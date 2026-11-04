import { useEffect, useState } from "react";
import { useTranslation } from "react-i18next";
import { Button } from "@/components/ui/button";
import { Input } from "@/components/ui/input";
import {
	Popover,
	PopoverContent,
	PopoverTrigger,
} from "@/components/ui/popover";
import { adminUserService } from "@/services/adminService";
import type { UserInfo } from "@/types/api";

interface AdminUserFilterProps {
	value?: number;
	onChange: (value?: number) => void;
}

export function AdminUserFilter({ value, onChange }: AdminUserFilterProps) {
	const { t } = useTranslation("admin");
	const [open, setOpen] = useState(false);
	const [keyword, setKeyword] = useState("");
	const [users, setUsers] = useState<UserInfo[]>([]);
	const [selected, setSelected] = useState<UserInfo | null>(null);
	const [loading, setLoading] = useState(false);

	useEffect(() => {
		if (value == null) {
			setSelected(null);
			return;
		}
		if (selected?.id === value) return;
		let active = true;
		adminUserService
			.get(value)
			.then((user) => {
				if (active) setSelected(user);
			})
			.catch(() => {
				if (active) setSelected(null);
			});
		return () => {
			active = false;
		};
	}, [value, selected?.id]);

	useEffect(() => {
		if (!open) return;
		let active = true;
		const timer = window.setTimeout(() => {
			setLoading(true);
			adminUserService
				.list({ keyword: keyword.trim() || undefined, limit: 20 })
				.then((page) => {
					if (active) setUsers(page.items);
				})
				.catch(() => {
					if (active) setUsers([]);
				})
				.finally(() => {
					if (active) setLoading(false);
				});
		}, 300);
		return () => {
			active = false;
			window.clearTimeout(timer);
		};
	}, [open, keyword]);

	return (
		<Popover open={open} onOpenChange={setOpen}>
			<PopoverTrigger
				render={
					<Button
						type="button"
						variant="outline"
						className="min-w-44 justify-between"
					/>
				}
			>
				{selected && selected.id === value
					? `${selected.username} (${selected.email})`
					: value == null
						? t("filter_user")
						: `#${value}`}
			</PopoverTrigger>
			<PopoverContent className="w-72">
				<Input
					aria-label={t("filter_user")}
					value={keyword}
					onChange={(event) => setKeyword(event.target.value)}
					placeholder={t("search_user")}
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
							{t("filter_all_users")}
						</Button>
					)}
					{loading ? (
						<p className="p-2 text-sm text-muted-foreground">
							{t("core:loading")}
						</p>
					) : (
						users.map((user) => (
							<Button
								key={user.id}
								type="button"
								variant="ghost"
								className="w-full justify-start truncate"
								onClick={() => {
									setSelected(user);
									onChange(user.id);
									setOpen(false);
								}}
							>
								{user.username} ({user.email})
							</Button>
						))
					)}
				</div>
			</PopoverContent>
		</Popover>
	);
}
