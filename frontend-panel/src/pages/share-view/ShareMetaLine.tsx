import { Fragment } from "react";

export function ShareMetaLine({
	className = "",
	items,
}: {
	className?: string;
	items: Array<string | null | undefined | false>;
}) {
	const visibleItems = items.filter(Boolean);
	return (
		<div
			className={`flex flex-wrap items-center gap-x-2 gap-y-1 text-sm text-muted-foreground ${className}`}
		>
			{visibleItems.map((item, index) => (
				<Fragment key={String(item)}>
					{index > 0 ? (
						<span className="text-muted-foreground/45">·</span>
					) : null}
					<span className="min-w-0">{item}</span>
				</Fragment>
			))}
		</div>
	);
}
