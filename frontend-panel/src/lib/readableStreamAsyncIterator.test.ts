import { describe, expect, it } from "vitest";
import { ensureReadableStreamAsyncIterator } from "./readableStreamAsyncIterator";

class FakeReadableStream<T> {
	#chunks: T[];
	lockReleased = false;
	cancelled = false;
	failOnCancel = false;

	constructor(chunks: T[]) {
		this.#chunks = [...chunks];
	}

	getReader() {
		return {
			read: async (): Promise<{
				done: boolean;
				value: T | undefined;
			}> => {
				const value = this.#chunks.shift();
				return value === undefined
					? { done: true, value: undefined }
					: { done: false, value };
			},
			cancel: async (): Promise<void> => {
				this.cancelled = true;
				if (this.failOnCancel) {
					throw new Error("cancel failed");
				}
			},
			releaseLock: () => {
				this.lockReleased = true;
			},
		};
	}
}

async function collect<T>(stream: AsyncIterable<T>): Promise<T[]> {
	const collected: T[] = [];
	for await (const value of stream) {
		collected.push(value);
	}
	return collected;
}

describe("ensureReadableStreamAsyncIterator", () => {
	it("installs Symbol.asyncIterator so for-await consumes every chunk", async () => {
		expect(ensureReadableStreamAsyncIterator(FakeReadableStream)).toBe(true);

		const stream = new FakeReadableStream(["a", "b", "c"]);
		const values = await collect(stream as unknown as AsyncIterable<string>);

		expect(values).toEqual(["a", "b", "c"]);
		expect(stream.cancelled).toBe(false);
		expect(stream.lockReleased).toBe(true);
	});

	it("cancels the stream and releases the lock when iteration terminates early", async () => {
		const stream = new FakeReadableStream([1, 2, 3]);
		const iterable = stream as unknown as AsyncIterable<number>;

		for await (const value of iterable) {
			expect(value).toBe(1);
			break;
		}

		expect(stream.cancelled).toBe(true);
		expect(stream.lockReleased).toBe(true);
	});

	it("swallows cancellation failures so they cannot mask the iteration outcome", async () => {
		const stream = new FakeReadableStream([1, 2, 3]);
		stream.failOnCancel = true;
		const iterable = stream as unknown as AsyncIterable<number>;

		for await (const value of iterable) {
			expect(value).toBe(1);
			break;
		}

		expect(stream.cancelled).toBe(true);
		expect(stream.lockReleased).toBe(true);
	});

	it("does not overwrite an existing async iterator", () => {
		const existing = async function* (): AsyncIterableIterator<unknown> {
			// Sentinel implementation that must survive the ensure call.
		};
		class AlreadyIterableStream {}
		Object.assign(AlreadyIterableStream.prototype, {
			[Symbol.asyncIterator]: existing,
		});

		expect(ensureReadableStreamAsyncIterator(AlreadyIterableStream)).toBe(
			false,
		);
		expect(
			(AlreadyIterableStream.prototype as unknown as AsyncIterable<unknown>)[
				Symbol.asyncIterator
			],
		).toBe(existing);
	});

	it("no-ops when the runtime exposes no ReadableStream at all", () => {
		expect(ensureReadableStreamAsyncIterator(null)).toBe(false);
	});
});
