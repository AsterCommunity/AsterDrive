import { UserIdentity } from "@/components/common/UserIdentity";
import type { UserSummary } from "@/types/api";

interface UserIdentityGroupProps {
	className?: string;
	fallbackLabel?: string;
	limit?: number;
	total?: number;
	users?: UserSummary[] | null;
}

export function UserIdentityGroup({
	className,
	fallbackLabel = "-",
	limit = 2,
	total,
	users,
}: UserIdentityGroupProps) {
	if (!users?.length) {
		if (total && total > 0) {
			return (
				<span className={className}>
					<span className="text-xs text-muted-foreground">+{total}</span>
				</span>
			);
		}
		return (
			<span className={className}>
				<span className="text-sm text-muted-foreground">{fallbackLabel}</span>
			</span>
		);
	}

	const visibleUsers = users.slice(0, limit);
	const remaining = Math.max(0, (total ?? users.length) - visibleUsers.length);

	return (
		<div className={className}>
			<div className="flex min-w-0 flex-col gap-1">
				{visibleUsers.map((user) => (
					<UserIdentity key={user.id} user={user} size="sm" />
				))}
				{remaining > 0 ? (
					<span className="text-xs text-muted-foreground">+{remaining}</span>
				) : null}
			</div>
		</div>
	);
}
