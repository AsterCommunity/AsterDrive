import { type SetStateAction, useCallback, useEffect, useState } from "react";
import { useApiList } from "@/hooks/useApiList";

export type ManagedAdminListQuery = {
	offset: number;
	pageSize: number;
};

export type ManagedAdminListPage<Item> = {
	items: Item[];
	total?: number;
};

export function useManagedAdminList<Item, Query extends ManagedAdminListQuery>({
	deps,
	loadPage,
	query,
	setOffset,
}: {
	deps: unknown[];
	loadPage: (query: Query) => Promise<ManagedAdminListPage<Item>>;
	query: Query;
	setOffset: (value: SetStateAction<number>) => void;
}) {
	const fetchPage = useCallback(
		() => loadPage(query),
		// biome-ignore lint/correctness/useExhaustiveDependencies: callers provide the managed query deps that should trigger a reload
		deps,
	);
	const { items, loading, reload, setItems, setTotal, total } = useApiList(
		fetchPage,
		deps,
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
