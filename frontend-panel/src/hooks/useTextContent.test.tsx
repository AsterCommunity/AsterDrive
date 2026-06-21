import { act, renderHook, waitFor } from "@testing-library/react";
import { beforeEach, describe, expect, it, vi } from "vitest";

const mockState = vi.hoisted(() => ({
	get: vi.fn(),
}));

vi.mock("@/services/http", () => ({
	api: {
		client: {
			get: mockState.get,
		},
	},
}));

vi.mock("@/lib/apiUrl", async () => {
	const actual =
		await vi.importActual<typeof import("@/lib/apiUrl")>("@/lib/apiUrl");
	return actual;
});

async function loadHookModule() {
	vi.resetModules();
	return await import("@/hooks/useTextContent");
}

describe("useTextContent", () => {
	beforeEach(() => {
		mockState.get.mockReset();
	});

	it("loads text content and etags", async () => {
		mockState.get.mockResolvedValue({
			status: 200,
			data: "hello world",
			headers: { etag: '"etag-1"' },
		});
		const { clearTextContentCache, useTextContent } = await loadHookModule();

		const { result } = renderHook(() => useTextContent("/files/1/content"));

		await waitFor(() => {
			expect(result.current.loading).toBe(false);
		});
		expect(result.current.content).toBe("hello world");
		expect(result.current.etag).toBe('"etag-1"');
		clearTextContentCache();
	});

	it("reuses cached content for 304 responses and honors local edits", async () => {
		mockState.get
			.mockResolvedValueOnce({
				status: 200,
				data: "original",
				headers: { etag: '"etag-1"' },
			})
			.mockResolvedValueOnce({
				status: 304,
				data: "",
				headers: {},
			});
		const { clearTextContentCache, useTextContent } = await loadHookModule();

		const first = renderHook(() => useTextContent("/files/1/content"));
		await waitFor(() => {
			expect(first.result.current.content).toBe("original");
		});

		act(() => {
			first.result.current.setContent("edited");
			first.result.current.setEtag('"etag-2"');
		});
		await waitFor(() => {
			expect(first.result.current.content).toBe("edited");
		});
		await waitFor(() => {
			expect(first.result.current.etag).toBe('"etag-2"');
		});
		first.unmount();

		const second = renderHook(() => useTextContent("/files/1/content"));
		await waitFor(() => {
			expect(second.result.current.content).toBe("edited");
		});

		expect(mockState.get).toHaveBeenNthCalledWith(2, "/files/1/content", {
			headers: { "If-None-Match": '"etag-2"' },
			responseType: "text",
			withCredentials: true,
			validateStatus: expect.any(Function),
		});
		expect(second.result.current.etag).toBe('"etag-2"');
		clearTextContentCache();
	});

	it("surfaces load failures as errors", async () => {
		mockState.get.mockRejectedValue(new Error("load failed"));
		const { clearTextContentCache, useTextContent } = await loadHookModule();

		const { result } = renderHook(() => useTextContent("/files/1/content"));

		await waitFor(() => {
			expect(result.current.error).toBe(true);
		});
		expect(result.current.loading).toBe(false);
		clearTextContentCache();
	});

	it("reloads fresh content without reusing the cached etag", async () => {
		mockState.get
			.mockResolvedValueOnce({
				status: 200,
				data: "original",
				headers: { etag: '"etag-1"' },
			})
			.mockResolvedValueOnce({
				status: 200,
				data: "refreshed",
				headers: { etag: '"etag-2"' },
			});
		const { clearTextContentCache, useTextContent } = await loadHookModule();

		const { result } = renderHook(() => useTextContent("/files/1/content"));
		await waitFor(() => {
			expect(result.current.content).toBe("original");
		});

		await act(async () => {
			await result.current.reload();
		});

		await waitFor(() => {
			expect(result.current.content).toBe("refreshed");
		});
		expect(result.current.etag).toBe('"etag-2"');
		expect(mockState.get).toHaveBeenNthCalledWith(2, "/files/1/content", {
			headers: {},
			responseType: "text",
			withCredentials: true,
			validateStatus: expect.any(Function),
		});
		clearTextContentCache();
	});

	it("keeps cached content visible when a revalidation request fails", async () => {
		mockState.get
			.mockResolvedValueOnce({
				status: 200,
				data: "cached",
				headers: { etag: '"etag-3"' },
			})
			.mockRejectedValueOnce(new Error("refresh failed"));
		const { clearTextContentCache, useTextContent } = await loadHookModule();

		const first = renderHook(() => useTextContent("/files/1/content"));
		await waitFor(() => {
			expect(first.result.current.content).toBe("cached");
		});
		first.unmount();

		const second = renderHook(() => useTextContent("/files/1/content"));
		await waitFor(() => {
			expect(second.result.current.error).toBe(true);
		});

		expect(second.result.current.content).toBe("cached");
		expect(mockState.get).toHaveBeenNthCalledWith(2, "/files/1/content", {
			headers: { "If-None-Match": '"etag-3"' },
			responseType: "text",
			withCredentials: true,
			validateStatus: expect.any(Function),
		});
		clearTextContentCache();
	});

	it("omits credentials for preview-link and external text resources", async () => {
		mockState.get.mockResolvedValue({
			status: 200,
			data: "preview",
			headers: { etag: '"etag-pv"' },
		});
		const { clearTextContentCache, useTextContent } = await loadHookModule();

		const preview = renderHook(() => useTextContent("/pv/token/file.txt"));
		await waitFor(() => {
			expect(preview.result.current.content).toBe("preview");
		});
		expect(mockState.get).toHaveBeenLastCalledWith("/pv/token/file.txt", {
			headers: {},
			responseType: "text",
			withCredentials: false,
			validateStatus: expect.any(Function),
		});

		mockState.get.mockClear();
		const external = renderHook(() =>
			useTextContent("https://objects.example.test/file.txt"),
		);
		await waitFor(() => {
			expect(external.result.current.content).toBe("preview");
		});
		expect(mockState.get).toHaveBeenLastCalledWith(
			"https://objects.example.test/file.txt",
			{
				headers: {},
				responseType: "text",
				withCredentials: false,
				validateStatus: expect.any(Function),
			},
		);
		clearTextContentCache();
	});

	it("re-fetches active consumers after invalidation", async () => {
		mockState.get
			.mockResolvedValueOnce({
				status: 200,
				data: "version-1",
				headers: { etag: '"etag-1"' },
			})
			.mockResolvedValueOnce({
				status: 200,
				data: "version-2",
				headers: { etag: '"etag-2"' },
			});
		const { clearTextContentCache, invalidateTextContent, useTextContent } =
			await loadHookModule();

		const { result } = renderHook(() => useTextContent("/files/1/content"));

		await waitFor(() => {
			expect(result.current.content).toBe("version-1");
		});

		act(() => {
			invalidateTextContent("/files/1/content");
		});

		await waitFor(() => {
			expect(mockState.get).toHaveBeenCalledTimes(2);
		});
		await waitFor(() => {
			expect(result.current.content).toBe("version-2");
		});
		expect(result.current.etag).toBe('"etag-2"');
		clearTextContentCache();
	});
});
