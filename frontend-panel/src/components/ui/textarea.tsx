import type { ComponentProps, Ref } from "react";

import {
	textControlBaseClasses,
	textControlStateClasses,
} from "@/components/ui/text-control-styles";
import { cn } from "@/lib/utils";

function Textarea({
	className,
	ref,
	...props
}: ComponentProps<"textarea"> & {
	ref?: Ref<HTMLTextAreaElement>;
}) {
	return (
		<textarea
			ref={ref}
			data-slot="textarea"
			data-theme-surface="control"
			className={cn(
				textControlBaseClasses,
				textControlStateClasses,
				"min-h-20 resize-y px-3 py-2",
				className,
			)}
			{...props}
		/>
	);
}

export { Textarea };
