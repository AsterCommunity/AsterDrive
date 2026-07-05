import { act, renderHook, waitFor } from "@testing-library/react";
import type { ReactNode } from "react";
import { MemoryRouter, useLocation, useSearchParams } from "react-router-dom";
import { describe, expect, it, vi } from "vitest";
import {
	type ManagedListQuerySchema,
	managedEnumQueryField,
	managedOptionalNumberQueryField,
	managedStringQueryField,
	useManagedListQueryState,
} from "@/hooks/useManagedListQueryState";
import {
	parseOffsetSearchParam,
	parsePageSizeSearchParam,
	parseSortOrderSearchParam,
	parseSortSearchParam,
	type SortOrder,
} from "@/lib/pagination";

type TestQuery = {
	offset: number;
	pageSize: 20 | 50;
	sortBy: "created_at" | "name";
	sortOrder: SortOrder;
	status: "all" | "active";
	text: string;
};

type FactoryQuery = {
	mode: "all" | "active";
	name: string;
	ownerId: number | undefined;
	raw: string;
	tag: string;
};

const TEST_DEFAULTS = {
	offset: 0,
	pageSize: 20,
	sortBy: "created_at",
	sortOrder: "desc",
	status: "all",
	text: "",
} satisfies TestQuery;

const TEST_SCHEMA: ManagedListQuerySchema<TestQuery> = {
	offset: {
		keys: ["offset"],
		parse: (params) => parseOffsetSearchParam(params.get("offset")),
		serialize: (value) => (value > 0 ? value : undefined),
	},
	pageSize: {
		keys: ["pageSize"],
		parse: (params) =>
			parsePageSizeSearchParam(params.get("pageSize"), [20, 50], 20),
		serialize: (value) => (value !== 20 ? value : undefined),
	},
	sortBy: {
		keys: ["sortBy"],
		parse: (params) =>
			parseSortSearchParam(
				params.get("sortBy"),
				["created_at", "name"],
				"created_at",
			),
		serialize: (value) => (value !== "created_at" ? value : undefined),
	},
	sortOrder: {
		keys: ["sortOrder"],
		parse: (params) =>
			parseSortOrderSearchParam(params.get("sortOrder"), "desc"),
		serialize: (value) => (value !== "desc" ? value : undefined),
	},
	status: {
		keys: ["status"],
		parse: (params) => (params.get("status") === "active" ? "active" : "all"),
		serialize: (value) => (value !== "all" ? value : undefined),
	},
	text: {
		keys: ["q"],
		parse: (params) => params.get("q") ?? "",
		serialize: (value) => ({ q: value.trim() || undefined }),
	},
};

const FACTORY_DEFAULTS = {
	mode: "all",
	name: "",
	ownerId: undefined,
	raw: "",
	tag: "",
} satisfies FactoryQuery;

const FACTORY_SCHEMA: ManagedListQuerySchema<FactoryQuery> = {
	mode: managedEnumQueryField({
		defaultValue: "all",
		key: "mode",
		options: ["all", "active"],
	}),
	name: managedStringQueryField({ key: "name" }),
	ownerId: managedOptionalNumberQueryField("ownerId"),
	raw: managedStringQueryField({ key: "raw", trimOnSerialize: false }),
	tag: {
		keys: ["tag"],
		parse: (params) => params.get("tag") ?? "",
		serialize: (value) => {
			const params = new URLSearchParams();
			if (value) {
				params.set("tag", value);
			}
			return params;
		},
	},
};

function createWrapper(initialEntry: string) {
	return function Wrapper({ children }: { children: ReactNode }) {
		return (
			<MemoryRouter initialEntries={[initialEntry]}>{children}</MemoryRouter>
		);
	};
}

function useTestQueryState() {
	const location = useLocation();
	const [searchParams, setSearchParams] = useSearchParams();
	const state = useManagedListQueryState({
		defaults: TEST_DEFAULTS,
		schema: TEST_SCHEMA,
		searchParams,
		setSearchParams,
	});

	return {
		...state,
		search: location.search,
	};
}

function useFactoryQueryState() {
	const location = useLocation();
	const [searchParams, setSearchParams] = useSearchParams();
	const state = useManagedListQueryState({
		defaults: FACTORY_DEFAULTS,
		schema: FACTORY_SCHEMA,
		searchParams,
		setSearchParams,
	});

	return {
		...state,
		search: location.search,
	};
}

describe("useManagedListQueryState", () => {
	it("reads invalid params through schema fallbacks and writes managed params back to the url", async () => {
		const { result } = renderHook(() => useTestQueryState(), {
			wrapper: createWrapper(
				"/admin?tab=files&offset=-2&pageSize=999&sortBy=nope&sortOrder=sideways&q= report ",
			),
		});

		expect(result.current.query).toEqual({
			...TEST_DEFAULTS,
			text: "report",
		});

		act(() => {
			result.current.setQuery({
				offset: 40,
				pageSize: 50,
				sortBy: "name",
				sortOrder: "asc",
				status: "active",
				text: " quarterly ",
			});
		});

		await waitFor(() => {
			const params = new URLSearchParams(result.current.search);
			expect(params.get("tab")).toBe("files");
			expect(params.get("offset")).toBe("40");
			expect(params.get("pageSize")).toBe("50");
			expect(params.get("sortBy")).toBe("name");
			expect(params.get("sortOrder")).toBe("asc");
			expect(params.get("status")).toBe("active");
			expect(params.get("q")).toBe("quarterly");
		});
	});

	it("omits default managed values while preserving unmanaged params", async () => {
		const { result } = renderHook(() => useTestQueryState(), {
			wrapper: createWrapper(
				"/admin?tab=files&offset=20&pageSize=50&sortBy=name&sortOrder=asc&status=active&q=report",
			),
		});

		act(() => {
			result.current.setQuery(TEST_DEFAULTS);
		});

		await waitFor(() => {
			const params = new URLSearchParams(result.current.search);
			expect(params.get("tab")).toBe("files");
			expect(params.has("offset")).toBe(false);
			expect(params.has("pageSize")).toBe(false);
			expect(params.has("sortBy")).toBe(false);
			expect(params.has("sortOrder")).toBe(false);
			expect(params.has("status")).toBe(false);
			expect(params.has("q")).toBe(false);
		});
	});

	it("uses replace navigation for query writes", () => {
		const setSearchParams = vi.fn();
		const { result } = renderHook(() =>
			useManagedListQueryState({
				defaults: TEST_DEFAULTS,
				schema: TEST_SCHEMA,
				searchParams: new URLSearchParams("tab=files&q=old"),
				setSearchParams,
			}),
		);

		act(() => {
			result.current.setQuery({ text: "new" });
		});

		expect(setSearchParams).toHaveBeenCalledTimes(1);
		const [nextParams, options] = setSearchParams.mock.calls[0];
		expect(nextParams.get("tab")).toBe("files");
		expect(nextParams.get("q")).toBe("new");
		expect(options).toEqual({ replace: true });
	});

	it("covers built-in field factories and URLSearchParams serialization", async () => {
		const { result } = renderHook(() => useFactoryQueryState(), {
			wrapper: createWrapper(
				"/admin?name=%20Alice%20&ownerId=invalid&mode=unknown&raw=%20keep%20&tag=blue",
			),
		});

		expect(result.current.query).toEqual({
			...FACTORY_DEFAULTS,
			name: "Alice",
			ownerId: undefined,
			raw: " keep ",
			tag: "blue",
		});

		act(() => {
			result.current.setQuery({
				mode: "active",
				name: " Bob ",
				ownerId: 42,
				raw: " keep spaces ",
				tag: "green",
			});
		});

		await waitFor(() => {
			const params = new URLSearchParams(result.current.search);
			expect(params.get("mode")).toBe("active");
			expect(params.get("name")).toBe("Bob");
			expect(params.get("ownerId")).toBe("42");
			expect(params.get("raw")).toBe(" keep spaces ");
			expect(params.get("tag")).toBe("green");
		});
	});

	it("can serialize enum defaults when requested", () => {
		const setSearchParams = vi.fn();
		type ModeQuery = { mode: "all" | "active" };
		const schema: ManagedListQuerySchema<ModeQuery> = {
			mode: managedEnumQueryField({
				defaultValue: "all",
				key: "mode",
				options: ["all", "active"],
				serializeDefault: true,
			}),
		};
		const { result } = renderHook(() =>
			useManagedListQueryState({
				defaults: { mode: "all" },
				schema,
				searchParams: new URLSearchParams(),
				setSearchParams,
			}),
		);
		setSearchParams.mockClear();

		act(() => {
			result.current.setQuery({ mode: "all" });
		});

		expect(setSearchParams).toHaveBeenCalledTimes(1);
		const [nextParams] = setSearchParams.mock.calls[0];
		expect(nextParams.get("mode")).toBe("all");
	});
});
