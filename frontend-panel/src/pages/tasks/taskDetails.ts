import type { TaskInfo } from "@/types/api";
import {
	parseStoragePolicyMigrationResult,
	parseTaskResult,
} from "./taskPresentation";

export function taskHasExpandableDetails(task: TaskInfo) {
	return (
		task.steps.length > 0 ||
		task.last_error !== null ||
		(task.status === "succeeded" &&
			(parseTaskResult(task) !== null ||
				parseStoragePolicyMigrationResult(task) !== null))
	);
}
