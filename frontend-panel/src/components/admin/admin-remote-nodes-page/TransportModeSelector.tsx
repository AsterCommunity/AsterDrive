import { useId } from "react";
import { cn } from "@/lib/utils";
import type { RemoteNodeTransportMode } from "../remoteNodePageShared";

export interface TransportModeOption {
	description: string;
	label: string;
	value: RemoteNodeTransportMode;
}

interface TransportModeSelectorProps {
	ariaLabelledBy: string;
	options: TransportModeOption[];
	value: RemoteNodeTransportMode;
	onChange: (value: RemoteNodeTransportMode) => void;
}

export function TransportModeSelector({
	ariaLabelledBy,
	options,
	value,
	onChange,
}: TransportModeSelectorProps) {
	const selectorId = useId();
	const inputName = `${selectorId}-transport-mode`;

	return (
		<div
			role="radiogroup"
			aria-labelledby={ariaLabelledBy}
			className="grid gap-2 md:grid-cols-3"
		>
			{options.map((option) => {
				const optionId = `${selectorId}-transport-mode-${option.value}`;
				const selected = value === option.value;

				return (
					<div key={option.value} className="min-w-0">
						<input
							id={optionId}
							type="radio"
							name={inputName}
							value={option.value}
							checked={selected}
							onChange={() => onChange(option.value)}
							className="peer sr-only"
						/>
						<label
							htmlFor={optionId}
							className={cn(
								"block min-h-24 cursor-pointer rounded-xl border p-3 text-left transition peer-focus-visible:border-ring peer-focus-visible:ring-3 peer-focus-visible:ring-ring/30",
								selected
									? "border-primary bg-primary/5"
									: "border-border/70 bg-background hover:border-primary/40",
							)}
						>
							<span className="flex items-center gap-2 text-sm font-semibold text-foreground">
								<span>{option.label}</span>
							</span>
							<span className="mt-1 block text-xs leading-5 text-muted-foreground">
								{option.description}
							</span>
						</label>
					</div>
				);
			})}
		</div>
	);
}
