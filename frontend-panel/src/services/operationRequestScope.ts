import type { AxiosRequestConfig } from "axios";

/**
 * 将操作所属登录生命周期绑定到每个 HTTP config。只在回调后检查账号还不够：
 * axios 的 401 interceptor 可能先拿新 cookie 重放旧请求。signal 必须保留在原 config，
 * 才能让取消在 refresh 等待、自动重放和业务 await 三个阶段都生效。
 */
export function bindOperationSignal<T extends object>(
	target: T,
	signal: AbortSignal,
): T {
	return new Proxy(target, {
		get(object, property) {
			const member = Reflect.get(object, property);
			switch (property) {
				case "client":
					return bindOperationSignal(member as object, signal);
				case "get":
				case "delete":
				case "post":
				case "put":
				case "patch":
					return (...args: unknown[]) => {
						signal.throwIfAborted();
						const index = property === "get" || property === "delete" ? 1 : 2;
						const config = (args[index] ?? {}) as AxiosRequestConfig;
						const previousSignal = config.signal as AbortSignal | undefined;
						args[index] = {
							...config,
							signal: previousSignal
								? AbortSignal.any([previousSignal, signal])
								: signal,
						};
						return Reflect.apply(
							member as (...arguments_: unknown[]) => unknown,
							object,
							args,
						);
					};
				default:
					return member;
			}
		},
	});
}
