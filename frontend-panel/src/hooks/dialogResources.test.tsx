import { act, render, screen, waitFor } from "@testing-library/react";
import { beforeEach, describe, expect, it, vi } from "vitest";
import { DialogLifecycle } from "@/components/ui/dialog-lifecycle";
import { useFileContentResource } from "@/hooks/useFileResource";
import {
	clearTextContentCache,
	invalidateTextContent,
	useTextContent,
} from "@/hooks/useTextContent";
import type { ReadyFileResourceHandle } from "@/lib/resourceRequest";

const mockState = vi.hoisted(() => ({ get: vi.fn() }));
vi.mock("@/services/http", () => ({ api: { client: { get: mockState.get } } }));

function ready(path: string): ReadyFileResourceHandle {
	return {
		kind: "ready",
		identity: { cacheKey: path, etag: null, scope: "personal" },
		request: {
			url: path,
			credentials: "include",
			conditionalHeaders: "allowed",
			redirectPolicy: "same_origin_only",
		},
		delivery: { mode: "text" },
	};
}

function Resource({
	active,
	resolve,
}: {
	active: boolean;
	resolve: () => Promise<ReadyFileResourceHandle>;
}) {
	const resource = useFileContentResource({
		open: active,
		downloadPath: "/test/content",
		fileId: 1,
		resolveResourceHandle: resolve,
	});
	const { content } = useTextContent(resource);
	return <div data-testid="content">{content ?? "loading"}</div>;
}

describe("dialog resource lifetime", () => {
	beforeEach(() => {
		clearTextContentCache();
		mockState.get
			.mockReset()
			.mockResolvedValue({ status: 200, data: "loaded preview", headers: {} });
	});

	it("retains loaded handles and text while stopping invalidation subscriptions on close", async () => {
		const resolve = vi.fn().mockResolvedValue(ready("/test/content"));
		let complete = (_open: boolean) => {};
		const view = (open: boolean) => (
			<DialogLifecycle open={open} target={null}>
				{({ active, complete: finish }) => {
					complete = finish;
					return <Resource active={active} resolve={resolve} />;
				}}
			</DialogLifecycle>
		);
		const { rerender } = render(view(true));
		await screen.findByText("loaded preview");
		rerender(view(false));
		expect(screen.getByText("loaded preview")).toBeInTheDocument();
		act(() => invalidateTextContent("/test/content"));
		expect(mockState.get).toHaveBeenCalledTimes(1);
		expect(screen.getByText("loaded preview")).toBeInTheDocument();
		act(() => complete(false));
		expect(screen.queryByTestId("content")).toBeNull();
	});

	it("discards a resolver that finishes during exit", async () => {
		let finish = (_resource: ReadyFileResourceHandle) => {};
		const promise = new Promise<ReadyFileResourceHandle>((resolve) => {
			finish = resolve;
		});
		const resolve = vi.fn(() => promise);
		const view = (open: boolean) => (
			<DialogLifecycle open={open} target={null}>
				{({ active }) => <Resource active={active} resolve={resolve} />}
			</DialogLifecycle>
		);
		const { rerender } = render(view(true));
		await waitFor(() => expect(resolve).toHaveBeenCalledTimes(1));
		rerender(view(false));
		await act(async () => {
			finish(ready("/late/content"));
			await promise;
		});
		expect(mockState.get).not.toHaveBeenCalled();
		expect(screen.getByText("loading")).toBeInTheDocument();
	});
});
