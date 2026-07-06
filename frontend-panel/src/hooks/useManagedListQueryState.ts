import { useCallback, useEffect, useMemo } from "react";
import { logger } from "@/lib/logger";
import {
	parseOffsetSearchParam,
	parsePageSizeSearchParam,
	parseSortOrderSearchParam,
	parseSortSearchParam,
	type SortOrder,
} from "@/lib/pagination";
import {
	buildQueryParams,
	type QueryParamRecord,
	type QueryParamValue,
} from "@/lib/queryParams";

type SetSearchParams = (
	nextInit: URLSearchParams,
	navigateOptions?: { replace?: boolean },
) => void;

export type ManagedListQueryField<Query extends object, Value> = {
	// Every URL parameter emitted by serialize must be listed here so the hook
	// can replace managed params without leaking stale values.
	keys: readonly string[];
	parse: (
		searchParams: URLSearchParams,
		defaults: Readonly<Partial<Query>>,
	) => Value;
	serialize?: (
		value: Value,
		query: Readonly<Query>,
		defaults: Readonly<Partial<Query>>,
	) => QueryParamRecord | QueryParamValue | URLSearchParams;
};

export type ManagedListQuerySchema<Query extends object> = {
	[Field in keyof Query]: ManagedListQueryField<Query, Query[Field]>;
};

export type ManagedListQueryUpdate<Query extends object> =
	| Partial<Query>
	| ((current: Readonly<Query>) => Partial<Query>);

export function managedOffsetQueryField<
	Query extends object,
>(): ManagedListQueryField<Query, number> {
	return {
		keys: ["offset"],
		parse: (searchParams) => parseOffsetSearchParam(searchParams.get("offset")),
		serialize: (value) => (value > 0 ? value : undefined),
	};
}

export function managedPageSizeQueryField<
	Query extends object,
	PageSize extends number,
>(
	pageSizeOptions: readonly PageSize[],
	defaultPageSize: PageSize,
): ManagedListQueryField<Query, PageSize> {
	return {
		keys: ["pageSize"],
		parse: (searchParams) =>
			parsePageSizeSearchParam(
				searchParams.get("pageSize"),
				pageSizeOptions,
				defaultPageSize,
			),
		serialize: (value) => (value !== defaultPageSize ? value : undefined),
	};
}

export function managedSortByQueryField<
	Query extends object,
	SortBy extends string,
>(
	sortOptions: readonly SortBy[],
	defaultSortBy: SortBy,
): ManagedListQueryField<Query, SortBy> {
	return {
		keys: ["sortBy"],
		parse: (searchParams) =>
			parseSortSearchParam(
				searchParams.get("sortBy"),
				sortOptions,
				defaultSortBy,
			),
		serialize: (value) => (value !== defaultSortBy ? value : undefined),
	};
}

export function managedSortOrderQueryField<Query extends object>(
	defaultSortOrder: SortOrder = "desc",
): ManagedListQueryField<Query, SortOrder> {
	return {
		keys: ["sortOrder"],
		parse: (searchParams) =>
			parseSortOrderSearchParam(
				searchParams.get("sortOrder"),
				defaultSortOrder,
			),
		serialize: (value) =>
			value !== defaultSortOrder ? { sortOrder: value } : undefined,
	};
}

export function managedStringQueryField<Query extends object>({
	key,
	trimOnSerialize = true,
}: {
	key: string;
	trimOnSerialize?: boolean;
}): ManagedListQueryField<Query, string> {
	return {
		keys: [key],
		parse: (searchParams) => {
			const value = searchParams.get(key) ?? "";
			return trimOnSerialize ? value.trim() : value;
		},
		serialize: (value) => {
			const nextValue = trimOnSerialize ? value.trim() : value;
			return nextValue || undefined;
		},
	};
}

export function managedOptionalNumberQueryField<Query extends object>(
	key: string,
): ManagedListQueryField<Query, number | undefined> {
	return {
		keys: [key],
		parse: (searchParams) => {
			const rawValue = searchParams.get(key);
			if (rawValue == null || rawValue.trim() === "") {
				return undefined;
			}
			const parsed = Number(rawValue);
			return Number.isSafeInteger(parsed) ? parsed : undefined;
		},
	};
}

export function managedEnumQueryField<
	Query extends object,
	Value extends string,
>({
	defaultValue,
	key,
	options,
	serializeDefault = false,
}: {
	defaultValue: Value;
	key: string;
	options: readonly Value[];
	serializeDefault?: boolean;
}): ManagedListQueryField<Query, Value> {
	return {
		keys: [key],
		parse: (searchParams) => {
			const rawValue = searchParams.get(key);
			return options.includes(rawValue as Value)
				? (rawValue as Value)
				: defaultValue;
		},
		serialize: (value) =>
			serializeDefault || value !== defaultValue ? value : undefined,
	};
}

function appendSerializedValue(
	query: URLSearchParams,
	keys: readonly string[],
	serialized: QueryParamRecord | QueryParamValue | URLSearchParams,
) {
	const allowedKeys = new Set(keys);
	const setSerializedParam = (key: string, value: string) => {
		if (!allowedKeys.has(key)) {
			logger.warn("Ignoring unmanaged query key from managed field", {
				key,
				managedKeys: keys,
			});
			return;
		}
		query.set(key, value);
	};

	if (serialized instanceof URLSearchParams) {
		for (const [key, value] of serialized.entries()) {
			setSerializedParam(key, value);
		}
		return;
	}

	if (typeof serialized === "object" && serialized !== null) {
		for (const [key, value] of buildQueryParams(serialized).entries()) {
			setSerializedParam(key, value);
		}
		return;
	}

	const [key] = keys;
	if (
		key &&
		serialized !== undefined &&
		serialized !== null &&
		serialized !== ""
	) {
		query.set(key, String(serialized));
	}
}

function mergeManagedSearchParams(
	currentSearchParams: URLSearchParams,
	nextManagedSearchParams: URLSearchParams,
	managedKeys: ReadonlySet<string>,
) {
	const merged = new URLSearchParams(currentSearchParams);
	for (const key of managedKeys) {
		merged.delete(key);
	}
	for (const [key, value] of nextManagedSearchParams.entries()) {
		merged.set(key, value);
	}
	return merged;
}

export function useManagedListQueryState<Query extends object>({
	defaults = {},
	schema,
	searchParams,
	setSearchParams,
}: {
	defaults?: Partial<Query>;
	schema: ManagedListQuerySchema<Query>;
	searchParams: URLSearchParams;
	setSearchParams: SetSearchParams;
}) {
	const fields = useMemo(
		() => Object.keys(schema) as Array<keyof Query>,
		[schema],
	);
	const managedKeys = useMemo(() => {
		const keys = new Set<string>();
		for (const field of fields) {
			for (const key of schema[field].keys) {
				keys.add(key);
			}
		}
		return keys;
	}, [fields, schema]);
	const query = useMemo(() => {
		const nextQuery = {} as Query;
		for (const field of fields) {
			nextQuery[field] = schema[field].parse(searchParams, defaults);
		}
		return nextQuery;
	}, [defaults, fields, schema, searchParams]);

	const buildManagedSearchParams = useCallback(
		(nextQuery: Query) => {
			const nextSearchParams = new URLSearchParams();
			for (const field of fields) {
				const fieldSchema = schema[field];
				const serialized = fieldSchema.serialize
					? fieldSchema.serialize(nextQuery[field], nextQuery, defaults)
					: (nextQuery[field] as QueryParamValue);
				appendSerializedValue(nextSearchParams, fieldSchema.keys, serialized);
			}
			return nextSearchParams;
		},
		[defaults, fields, schema],
	);

	useEffect(() => {
		const normalizedSearchParams = mergeManagedSearchParams(
			searchParams,
			buildManagedSearchParams(query),
			managedKeys,
		);
		if (normalizedSearchParams.toString() === searchParams.toString()) {
			return;
		}

		setSearchParams(normalizedSearchParams, { replace: true });
	}, [
		buildManagedSearchParams,
		managedKeys,
		query,
		searchParams,
		setSearchParams,
	]);

	const setQuery = useCallback(
		(updates: ManagedListQueryUpdate<Query>) => {
			const patch = typeof updates === "function" ? updates(query) : updates;
			const nextQuery = { ...query, ...patch };
			const nextManagedSearchParams = buildManagedSearchParams(nextQuery);
			setSearchParams(
				mergeManagedSearchParams(
					searchParams,
					nextManagedSearchParams,
					managedKeys,
				),
				{ replace: true },
			);
		},
		[
			buildManagedSearchParams,
			managedKeys,
			query,
			searchParams,
			setSearchParams,
		],
	);

	return {
		query,
		setQuery,
	};
}
