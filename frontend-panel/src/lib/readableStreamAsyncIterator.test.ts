import { describe, expect, it } from "vitest";
import { ensureReadableStreamAsyncIterator } from "./readableStreamAsyncIterator";

class FakeReadableStream<T> {
	#chunks: T[];
	lockReleased = false;

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
		expect(stream.lockReleased).toBe(true);
	});

	it("releases the reader lock when iteration terminates early", async () => {
		const stream = new FakeReadableStream([1, 2, 3]);
		const iterable = stream as unknown as AsyncIterable<number>;

		for await (const value of iterable) {
			expect(value).toBe(1);
			break;
		}

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
