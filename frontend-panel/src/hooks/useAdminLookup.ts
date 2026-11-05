import { useContext, useEffect, useMemo, useRef, useState } from "react";
import { AdminLookupContext } from "@/hooks/adminLookupContext";
import { useDebouncedValue } from "@/hooks/useDebouncedValue";
import type {
	AdminLookupKind,
	AdminLookupOption,
} from "@/services/adminLookupService";

export function useAdminLookup(
	kind: AdminLookupKind,
	value: number | undefined,
	open: boolean,
) {
	const resource = useContext(AdminLookupContext);
	if (!resource) throw new Error("Admin lookup requires AdminLookupProvider");
	const [search, setSearch] = useState("");
	const keyword = useDebouncedValue(search);
	const [page, setPage] = useState({
		keyword: "",
		items: [] as AdminLookupOption[],
		total: 0,
	});
	const [offset, setOffset] = useState(0);
	const [selected, setSelected] = useState<AdminLookupOption | null>(null);
	const [loading, setLoading] = useState(false);
	const [selectedLoading, setSelectedLoading] = useState(value != null);
	const [searchFailed, setSearchFailed] = useState(false);
	const [selectedFailed, setSelectedFailed] = useState(false);
	const [retry, setRetry] = useState(0);
	const identityRetry = useRef(0);
	const searchRetry = useRef(0);
	useEffect(() => {
		setSelectedFailed(false);
		if (value == null) {
			setSelected(null);
			setSelectedLoading(false);
			return;
		}
		const controller = new AbortController();
		setSelected((current) => (current?.id === value ? current : null));
		setSelectedLoading(true);
		const force = identityRetry.current !== retry;
		identityRetry.current = retry;
		resource
			.get(kind, value, controller.signal, force)
			.then((option) => {
				if (!controller.signal.aborted) setSelected(option);
			})
			.catch(() => {
				if (!controller.signal.aborted) setSelectedFailed(true);
			})
			.finally(() => {
				if (!controller.signal.aborted) setSelectedLoading(false);
			});
		return () => controller.abort();
	}, [kind, value, retry, resource]);
	useEffect(() => {
		if (!open || search !== keyword) return;
		const controller = new AbortController();
		setLoading(true);
		setSearchFailed(false);
		const force = searchRetry.current !== retry;
		searchRetry.current = retry;
		resource
			.search(kind, keyword, offset, controller.signal, force)
			.then((result) => {
				if (controller.signal.aborted) return;
				setPage((previous) => ({
					keyword,
					items:
						offset > 0 && previous.keyword === keyword
							? [...previous.items.slice(0, offset), ...result.items]
							: result.items,
					total: result.total,
				}));
			})
			.catch(() => {
				if (!controller.signal.aborted) setSearchFailed(true);
			})
			.finally(() => {
				if (!controller.signal.aborted) setLoading(false);
			});
		return () => controller.abort();
	}, [kind, open, keyword, search, offset, retry, resource]);
	const options = useMemo(() => {
		const items = page.keyword === search ? page.items : [];
		const current = selected?.id === value ? selected : null;
		return current && !items.some((item) => item.id === current.id)
			? [current, ...items]
			: items;
	}, [page, search, selected, value]);
	return {
		options,
		selectedLoading,
		rememberSelection: (option: AdminLookupOption | null) => {
			setSelected(option);
			setSelectedFailed(false);
		},
		selected: selected?.id === value ? selected : null,
		search,
		setSearch: (value: string) => {
			setSearch(value);
			setOffset(0);
		},
		loading: loading || search !== keyword,
		failed: searchFailed || selectedFailed,
		hasMore: page.keyword === search && page.items.length < page.total,
		loadMore: () => setOffset(page.items.length),
		retry: () => setRetry((value) => value + 1),
	};
}
