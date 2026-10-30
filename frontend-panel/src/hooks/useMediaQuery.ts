import { useEffect, useState } from "react";

export function useMediaQuery(query: string) {
	const getMatches = () =>
		typeof window !== "undefined" &&
		typeof window.matchMedia === "function" &&
		window.matchMedia(query).matches;

	const [matches, setMatches] = useState(getMatches);

	useEffect(() => {
		if (
			typeof window === "undefined" ||
			typeof window.matchMedia !== "function"
		) {
			return;
		}

		const mediaQuery = window.matchMedia(query);
		setMatches(mediaQuery.matches);

		const handleChange = (event: MediaQueryListEvent) => {
			setMatches(event.matches);
		};

		if (typeof mediaQuery.addEventListener === "function") {
			mediaQuery.addEventListener("change", handleChange);
			return () => mediaQuery.removeEventListener("change", handleChange);
		}

		mediaQuery.addListener(handleChange);
		return () => mediaQuery.removeListener(handleChange);
	}, [query]);

	return matches;
}
