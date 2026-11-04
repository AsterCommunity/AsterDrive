import { useState } from "react";
import { AdminDebouncedFilterInput } from "@/components/admin/AdminDebouncedFilterInput";
import {
	Select,
	SelectContent,
	SelectItem,
	SelectTrigger,
	SelectValue,
} from "@/components/ui/select";

const UNITS = ["B", "KB", "MB", "GB", "TB"] as const;
type SizeUnit = (typeof UNITS)[number];

function initialUnit(bytes?: number): SizeUnit {
	if (bytes == null || bytes === 0) return "B";
	for (let index = UNITS.length - 1; index > 0; index--) {
		if (bytes % 1024 ** index === 0) return UNITS[index];
	}
	return "B";
}

export function AdminSizeFilter({
	bytes,
	label,
	onChange,
}: {
	bytes?: number;
	label: string;
	onChange: (bytes?: number) => void;
}) {
	const [unit, setUnit] = useState<SizeUnit>(() => initialUnit(bytes));
	const factor = 1024 ** UNITS.indexOf(unit);
	const display = bytes == null ? "" : String(bytes / factor);
	return (
		<div className="flex h-9 items-center gap-1">
			<AdminDebouncedFilterInput
				aria-label={label}
				placeholder={label}
				className="w-28"
				inputMode="decimal"
				value={display}
				onChange={(text) => {
					if (!text.trim()) {
						onChange(undefined);
						return;
					}
					const next = Number(text);
					if (
						Number.isFinite(next) &&
						next >= 0 &&
						Number.isSafeInteger(Math.round(next * factor))
					) {
						onChange(Math.round(next * factor));
					}
				}}
			/>
			<Select
				value={unit}
				onValueChange={(value) => {
					if (UNITS.includes(value as SizeUnit)) setUnit(value as SizeUnit);
				}}
			>
				<SelectTrigger aria-label={`${label} unit`} width="compact">
					<SelectValue>{unit}</SelectValue>
				</SelectTrigger>
				<SelectContent>
					{UNITS.map((option) => (
						<SelectItem key={option} value={option}>
							{option}
						</SelectItem>
					))}
				</SelectContent>
			</Select>
		</div>
	);
}
