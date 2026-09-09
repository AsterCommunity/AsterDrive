import type { ComponentProps } from "react";
import { Label } from "@/components/ui/label";
import { cn } from "@/lib/utils";

interface FormFieldLabelProps extends ComponentProps<typeof Label> {
	required?: boolean;
}

export function FormFieldLabel({
	children,
	required = false,
	...props
}: FormFieldLabelProps) {
	return (
		<Label
			{...props}
			className={cn(
				props.className,
				required &&
					"gap-0 after:ml-0.5 after:text-destructive after:content-['*']",
			)}
		>
			{children}
		</Label>
	);
}
