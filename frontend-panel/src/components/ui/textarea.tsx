import type { ComponentProps } from "react";
import {
	textControlClasses,
	textControlFocusClasses,
} from "@/components/ui/text-control-styles";
import { cn } from "@/lib/utils";

function Textarea({ className, ...props }: ComponentProps<"textarea">) {
	return (
		<textarea
			data-slot="textarea"
			data-theme-surface="control"
			className={cn(
				textControlClasses,
				textControlFocusClasses,
				"min-h-24 resize-y px-3 py-2",
				className,
			)}
			{...props}
		/>
	);
}

export { Textarea };
