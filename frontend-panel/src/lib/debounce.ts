/** Keeps only the latest pending task; cancellation never stops a running task. */
export function createDebouncedTask(delay: number) {
	let timer: ReturnType<typeof setTimeout> | undefined;
	const cancel = () => {
		if (timer !== undefined) {
			clearTimeout(timer);
			timer = undefined;
		}
	};
	return {
		cancel,
		schedule(task: () => void) {
			cancel();
			timer = setTimeout(() => {
				timer = undefined;
				task();
			}, delay);
		},
	};
}
