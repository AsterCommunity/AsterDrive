import { buildQueryParams, type QueryParamRecord } from "@/lib/queryParams";

export function parseOffsetSearchParam(rawValue: string | null): number {
	const parsed = Number(rawValue ?? "0");
	if (!Number.isFinite(parsed) || parsed < 0 || !Number.isInteger(parsed)) {
		return 0;
	}

	return Math.min(parsed, Number.MAX_SAFE_INTEGER);
}

export function parsePageSizeSearchParam<PageSize extends number>(
	rawValue: string | null,
	pageSizeOptions: readonly PageSize[],
	defaultPageSize: PageSize,
): PageSize {
	const parsed = Number(rawValue ?? String(defaultPageSize));

	return pageSizeOptions.includes(parsed as PageSize)
		? (parsed as PageSize)
		: defaultPageSize;
}

export function parsePageSizeOption<PageSize extends number>(
	value: string | null,
	pageSizeOptions: readonly PageSize[],
): PageSize | null {
	if (!value) {
		return null;
	}

	const parsed = Number(value);
	return pageSizeOptions.includes(parsed as PageSize)
		? (parsed as PageSize)
		: null;
}

export type SortOrder = "asc" | "desc";

export function parseSortSearchParam<SortBy extends string>(
	rawValue: string | null,
	options: readonly SortBy[],
	defaultValue: SortBy,
): SortBy {
	return options.includes(rawValue as SortBy)
		? (rawValue as SortBy)
		: defaultValue;
}

export function parseSortOrderSearchParam(
	rawValue: string | null,
	defaultValue: SortOrder,
): SortOrder {
	return rawValue === "asc" || rawValue === "desc" ? rawValue : defaultValue;
}

export function buildOffsetPaginationSearchParams<PageSize extends number>({
	offset,
	pageSize,
	defaultPageSize,
	extraParams,
}: {
	offset: number;
	pageSize: PageSize;
	defaultPageSize: PageSize;
	extraParams?: QueryParamRecord;
}): URLSearchParams {
	const query = buildQueryParams(extraParams);
	query.delete("offset");
	query.delete("pageSize");

	if (offset > 0) {
		query.set("offset", String(offset));
	}
	if (pageSize !== defaultPageSize) {
		query.set("pageSize", String(pageSize));
	}

	return query;
}
