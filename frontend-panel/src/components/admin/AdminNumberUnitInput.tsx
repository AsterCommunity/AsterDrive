import { useTranslation } from "react-i18next";
import { Input } from "@/components/ui/input";
import { Label } from "@/components/ui/label";
import {
	Select,
	SelectContent,
	SelectItem,
	SelectTrigger,
	SelectValue,
} from "@/components/ui/select";
import { ADMIN_CONTROL_HEIGHT_CLASS } from "@/lib/constants";
import type { NumberUnitOption } from "@/lib/numberUnit";
import { cn } from "@/lib/utils";

interface AdminNumberUnitInputProps<TValue extends string> {
	className?: string;
	disabled?: boolean;
	errorMessage?: string | null;
	id?: string;
	inputClassName?: string;
	invalid?: boolean;
	label?: string;
	onUnitChange: (value: TValue) => void;
	onValueChange: (value: string) => void;
	placeholder: string;
	unit: TValue;
	unitAriaLabel: string;
	unitClassName?: string;
	units: ReadonlyArray<NumberUnitOption<TValue>>;
	value: string;
}

export function AdminNumberUnitInput<TValue extends string>({
	className,
	disabled,
	errorMessage,
	id,
	inputClassName,
	invalid,
	label,
	onUnitChange,
	onValueChange,
	placeholder,
	unit,
	unitAriaLabel,
	unitClassName,
	units,
	value,
}: AdminNumberUnitInputProps<TValue>) {
	const { t } = useTranslation("admin");
	const invalidState = Boolean(errorMessage) || invalid === true;

	return (
		<div className={cn("space-y-2", className)}>
			{label ? <Label htmlFor={id}>{label}</Label> : null}
			<div className="flex flex-col gap-2 sm:flex-row sm:items-center">
				<Input
					id={id}
					type="number"
					inputMode="numeric"
					min={0}
					step={1}
					value={value}
					disabled={disabled}
					aria-invalid={invalidState ? true : undefined}
					placeholder={placeholder}
					className={cn(
						ADMIN_CONTROL_HEIGHT_CLASS,
						"min-w-0 flex-1",
						invalidState && "border-destructive focus-visible:ring-destructive",
						inputClassName,
					)}
					onChange={(event) => onValueChange(event.target.value)}
				/>
				<Select
					items={units.map((option) => ({
						label: t(option.labelKey),
						value: option.value,
					}))}
					value={unit}
					onValueChange={(nextValue) => {
						const matchedUnit = units.find(
							(option) => option.value === nextValue,
						);
						if (matchedUnit) {
							onUnitChange(matchedUnit.value);
						}
					}}
					disabled={disabled}
				>
					<SelectTrigger
						width="fit"
						className={cn(
							ADMIN_CONTROL_HEIGHT_CLASS,
							"min-w-28",
							invalidState &&
								"border-destructive focus-visible:ring-destructive",
							unitClassName,
						)}
						aria-label={unitAriaLabel}
						aria-invalid={invalidState ? true : undefined}
					>
						<SelectValue />
					</SelectTrigger>
					<SelectContent align="end">
						{units.map((option) => (
							<SelectItem key={option.value} value={option.value}>
								{t(option.labelKey)}
							</SelectItem>
						))}
					</SelectContent>
				</Select>
			</div>
			{errorMessage ? (
				<p className="text-xs text-destructive">{errorMessage}</p>
			) : null}
		</div>
	);
}
