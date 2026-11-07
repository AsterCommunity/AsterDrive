import type { Workspace } from "@/lib/workspace";

/**
 * 本地异步操作的身份边界。generation 只随登录身份生命周期变化，正常 refresh
 * 不变化；AbortSignal 同时阻止已经发送的请求在 401 refresh 后被新账号重放。
 * 它不能代替服务端授权，也不保存 token、密码或可持久化的授权决定。
 */
export interface AuthOperationOwner {
	readonly accountId: number;
	readonly generation: number;
	readonly signal: AbortSignal;
}

export interface UploadOperationBinding extends AuthOperationOwner {
	readonly workspace: Workspace;
}

let accountId: number | null = null;
let generation = 0;
let controller = new AbortController();

export function setAuthOperationOwner(
	nextAccountId: number | null,
	force = false,
) {
	if (nextAccountId === accountId && !force) return;
	controller.abort();
	generation += 1;
	controller = new AbortController();
	accountId = nextAccountId;
}

export function getAuthOperationGeneration() {
	return generation;
}

export function captureAuthOperationOwner(): AuthOperationOwner {
	if (accountId === null) {
		throw new DOMException(
			"No active account owns this operation",
			"AbortError",
		);
	}
	return Object.freeze({ accountId, generation, signal: controller.signal });
}

export function captureUploadOperation(
	workspace: Workspace,
	lifetimeSignal?: AbortSignal,
): UploadOperationBinding {
	const owner = captureAuthOperationOwner();
	return Object.freeze({
		...owner,
		signal: lifetimeSignal
			? AbortSignal.any([owner.signal, lifetimeSignal])
			: owner.signal,
		workspace: Object.freeze({ ...workspace }),
	});
}

export function isCurrentAuthOperation(owner: AuthOperationOwner): boolean {
	return (
		owner.accountId === accountId &&
		owner.generation === generation &&
		!owner.signal.aborted
	);
}

export function assertCurrentAuthOperation(owner: AuthOperationOwner) {
	if (!isCurrentAuthOperation(owner)) {
		throw new DOMException(
			"The operation belongs to a previous login",
			"AbortError",
		);
	}
}
