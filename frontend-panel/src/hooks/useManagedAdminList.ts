import { type SetStateAction, useCallback, useEffect, useState } from "react";
import { useApiList } from "@/hooks/useApiList";
import type { ManagedListQueryUpdate } from "@/hooks/useManagedListQueryState";

export type ManagedAdminListQuery = {
	offset: number;
	pageSize: number;
};

export type ManagedAdminListPage<Item> = {
	items: Item[];
	total?: number;
};

type ManagedListQuerySetter<Query extends ManagedAdminListQuery> = (
	updates: ManagedListQueryUpdate<Query>,
) => void;

function normalizeManagedOffset(offset: number) {
	return Math.max(0, Math.floor(offset));
}

export function useManagedOffset<Query extends ManagedAdminListQuery>(
	setQuery: ManagedListQuerySetter<Query>,
	normalize: (offset: number) => number = normalizeManagedOffset,
) {
	return useCallback(
		(value: SetStateAction<number>) => {
			setQuery(
				(current) =>
					({
						offset: normalize(
							typeof value === "function" ? value(current.offset) : value,
						),
					}) as Partial<Query>,
			);
		},
		[normalize, setQuery],
	);
}

export function useManagedAdminList<Item, Query extends ManagedAdminListQuery>({
	deps = [],
	loadPage,
	query,
	setOffset,
}: {
	// Extra dependencies for loadPage values that are not represented in query.
	deps?: unknown[];
	loadPage: (query: Query) => Promise<ManagedAdminListPage<Item>>;
	query: Query;
	setOffset: (value: SetStateAction<number>) => void;
}) {
	const reloadDeps = [...Object.values(query), ...deps];
	const fetchPage = useCallback(
		() => loadPage(query),
		// biome-ignore lint/correctness/useExhaustiveDependencies: managed query values plus explicit extra deps define reload boundaries
		reloadDeps,
	);
	const { items, loading, reload, setItems, setTotal, total } = useApiList(
		fetchPage,
		reloadDeps,
	);
	const { offset, pageSize } = query;
	const totalPages = Math.max(1, Math.ceil(total / pageSize));
	const currentPage = Math.floor(offset / pageSize) + 1;

	useEffect(() => {
		if (loading || items.length > 0 || total <= 0) {
			return;
		}

		const maxOffset = Math.floor((total - 1) / pageSize) * pageSize;
		if (offset > maxOffset) {
			setOffset(maxOffset);
		}
	}, [items.length, loading, offset, pageSize, setOffset, total]);

	return {
		currentPage,
		items,
		loading,
		nextPageDisabled: offset + pageSize >= total,
		prevPageDisabled: offset === 0,
		reload,
		setItems,
		setTotal,
		total,
		totalPages,
	};
}

export function useManagedAdminListDetailDialog<Detail>() {
	const [detail, setDetail] = useState<Detail | null>(null);
	const handleOpenChange = useCallback((open: boolean) => {
		if (!open) {
			setDetail(null);
		}
	}, []);

	return {
		detail,
		open: detail !== null,
		onOpenChange: handleOpenChange,
		setDetail,
	};
}
