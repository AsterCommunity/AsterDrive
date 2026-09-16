interface ReadableStreamPrototypeLike {
	getReader(): ReadableStreamDefaultReader;
	[Symbol.asyncIterator]?:
		| ((this: ReadableStreamPrototypeLike) => AsyncIterableIterator<unknown>)
		| undefined;
}

interface ReadableStreamConstructorLike {
	prototype: object;
}

/**
 * Installs `ReadableStream.prototype[Symbol.asyncIterator]` when the runtime
 * lacks it, so `for await...of` works over web streams.
 *
 * WebKit (Safari — i.e. every browser on iOS) only added the async iterator
 * in Safari 26.4, while pdf.js 6.x consumes text-content streams with
 * `for await...of`. Without this shim `getTextContent()` throws
 * `TypeError: undefined is not a function`, which kills the text layer and
 * with it page rendering on iOS. Upstream declined to fix it
 * (mozilla/pdf.js#20973, #21557, #21924) and recommends consumers polyfill.
 *
 * @returns `true` when the shim was installed, `false` when skipped because
 * the runtime already supports it or exposes no `ReadableStream` at all.
 */
export function ensureReadableStreamAsyncIterator(
	streamConstructor: ReadableStreamConstructorLike | null = typeof ReadableStream ===
	"undefined"
		? null
		: ReadableStream,
): boolean {
	if (!streamConstructor) return false;

	const prototype = streamConstructor.prototype as ReadableStreamPrototypeLike;
	if (typeof prototype[Symbol.asyncIterator] === "function") return false;

	prototype[Symbol.asyncIterator] = async function* () {
		const reader = this.getReader();
		try {
			for (;;) {
				const { done, value } = await reader.read();
				if (done) return;
				yield value;
			}
		} finally {
			reader.releaseLock();
		}
	};
	return true;
}
