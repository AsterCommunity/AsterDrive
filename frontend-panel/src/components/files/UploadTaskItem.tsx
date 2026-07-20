import { TransferTaskItem } from "@/components/files/TransferActivitySection";
import { Button } from "@/components/ui/button";
import { Icon } from "@/components/ui/icon";

interface UploadTaskAction {
	label: string;
	icon: "X" | "ArrowsClockwise" | "Upload";
	onClick: () => void;
	variant?: "outline" | "ghost";
}

interface UploadTaskItemProps {
	title: string;
	status: string;
	mode: string;
	progress: number;
	detail?: string;
	speed?: string;
	completed?: boolean;
	cancelled?: boolean;
	actions?: UploadTaskAction[];
}

const EMPTY_UPLOAD_TASK_ACTIONS: UploadTaskAction[] = [];

export function UploadTaskItem({
	title,
	status,
	mode,
	progress,
	detail,
	speed,
	completed = false,
	cancelled = false,
	actions = EMPTY_UPLOAD_TASK_ACTIONS,
}: UploadTaskItemProps) {
	const failed =
		!completed &&
		!cancelled &&
		actions.some((action) => action.icon === "ArrowsClockwise");
	const waitingForFile =
		!completed &&
		!cancelled &&
		actions.some((action) => action.icon === "Upload");
	const showProgress =
		!completed && !cancelled && !failed && !waitingForFile && progress < 100;
	const icon = completed
		? ("Check" as const)
		: cancelled
			? ("X" as const)
			: failed
				? ("CircleAlert" as const)
				: waitingForFile
					? ("Upload" as const)
					: ("Spinner" as const);
	const tone = completed
		? ("success" as const)
		: failed || cancelled
			? ("error" as const)
			: showProgress || waitingForFile
				? ("active" as const)
				: ("default" as const);
	const detailText = [mode, detail ?? status, speed]
		.filter(Boolean)
		.join(" · ");

	return (
		<TransferTaskItem
			title={title}
			detail={detailText}
			icon={icon}
			tone={tone}
			progress={showProgress ? progress : null}
			progressLabel={showProgress ? `${progress}%` : undefined}
			actions={
				actions.length > 0
					? actions.map((action) => (
							<Button
								key={`${action.icon}-${action.label}`}
								variant={action.variant ?? "ghost"}
								size="icon-xs"
								onClick={action.onClick}
								aria-label={action.label}
								title={action.label}
							>
								<Icon name={action.icon} className="size-3" />
							</Button>
						))
					: undefined
			}
		/>
	);
}
