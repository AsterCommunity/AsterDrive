import { useTranslation } from "react-i18next";
import { cn } from "@/lib/utils";
import type { ExternalAuthCreateStep } from "./shared";

interface ExternalAuthCreateProgressProps {
	createStep: number;
	createSteps: ExternalAuthCreateStep[];
	onCreateStepChange: (step: number) => void;
}

export function ExternalAuthCreateProgress({
	createStep,
	createSteps,
	onCreateStepChange,
}: ExternalAuthCreateProgressProps) {
	const { t } = useTranslation("admin");
	const currentStep = createSteps[Math.min(createStep, createSteps.length - 1)];

	return (
		<div className="space-y-3">
			<div className="rounded-xl bg-muted/30 p-3 sm:p-4">
				<div className="flex items-start justify-between gap-3">
					<div className="space-y-1">
						<p className="text-[11px] font-medium uppercase tracking-[0.2em] text-muted-foreground">
							{t("policy_wizard_progress", {
								current: createStep + 1,
								total: createSteps.length,
							})}
						</p>
						<h3 className="text-sm font-semibold sm:text-base">
							{currentStep.title}
						</h3>
						<p className="hidden text-sm text-muted-foreground sm:block">
							{currentStep.description}
						</p>
					</div>
					<div className="hidden text-3xl leading-none font-semibold text-foreground/15 md:block">
						{String(createStep + 1).padStart(2, "0")}
					</div>
				</div>
				<div className="mt-4 h-1.5 overflow-hidden rounded-full bg-background/80">
					<div
						className="h-full rounded-full bg-primary transition-[width] duration-300"
						style={{
							width: `${((createStep + 1) / createSteps.length) * 100}%`,
						}}
					/>
				</div>
			</div>

			<div className="hidden gap-2 md:grid md:grid-cols-3">
				{createSteps.map((step, index) => (
					<button
						type="button"
						key={step.title}
						disabled={index > createStep}
						onClick={() => onCreateStepChange(index)}
						className={cn(
							"flex items-center gap-3 rounded-xl p-3 text-left transition-colors focus-visible:outline-none focus-visible:ring-3 focus-visible:ring-ring/30",
							index === createStep
								? "bg-primary/10 text-foreground"
								: index < createStep
									? "bg-muted/50 hover:bg-muted/70"
									: "bg-muted/20 text-muted-foreground",
						)}
					>
						<span className="flex size-6 shrink-0 items-center justify-center rounded-full bg-background/80 text-[10px] font-semibold tracking-[0.16em] text-muted-foreground shadow-xs">
							{index + 1}
						</span>
						<span className="text-sm font-medium leading-5">{step.title}</span>
					</button>
				))}
			</div>
		</div>
	);
}
