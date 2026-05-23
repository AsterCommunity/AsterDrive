import { render, screen } from "@testing-library/react";
import { describe, expect, it, vi } from "vitest";
import { SessionPlatformIcon } from "@/components/settings/SessionPlatformIcon";

vi.mock("react-icons/fa6", () => {
	const makeIcon =
		(name: string) =>
		({ className }: { className?: string }) => (
			<span data-testid={name} className={className} />
		);

	return {
		FaAndroid: makeIcon("fa-android"),
		FaApple: makeIcon("fa-apple"),
		FaChrome: makeIcon("fa-chrome"),
		FaDisplay: makeIcon("fa-display"),
		FaEdge: makeIcon("fa-edge"),
		FaFirefoxBrowser: makeIcon("fa-firefox"),
		FaLinux: makeIcon("fa-linux"),
		FaMobileScreenButton: makeIcon("fa-mobile"),
		FaOpera: makeIcon("fa-opera"),
		FaSafari: makeIcon("fa-safari"),
		FaTabletScreenButton: makeIcon("fa-tablet"),
		FaWindows: makeIcon("fa-windows"),
	};
});

vi.mock("react-icons/pi", () => {
	const makeIcon =
		(name: string) =>
		({ className }: { className?: string }) => (
			<span data-testid={name} className={className} />
		);

	return {
		PiBrowsers: makeIcon("pi-browsers"),
		PiDesktop: makeIcon("pi-desktop"),
		PiGlobeHemisphereWest: makeIcon("pi-globe"),
	};
});

describe("SessionPlatformIcon", () => {
	it("renders browser and platform icons for a recognized edge mac session", () => {
		render(
			<SessionPlatformIcon userAgent="Mozilla/5.0 (Macintosh; Intel Mac OS X 10_15_7) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/147.0.0.0 Safari/537.36 Edg/147.0.0.0" />,
		);

		expect(screen.getByTestId("fa-edge")).toHaveClass("size-4", "text-sky-600");
		expect(screen.getByTestId("fa-apple")).toHaveClass(
			"h-2.5",
			"w-2.5",
			"text-slate-700",
		);
	});

	it("falls back to a generic browser icon when the browser is unknown", () => {
		render(
			<SessionPlatformIcon userAgent="CustomAgent/1.0 (Linux; Android 14; Pixel 8) Mobile" />,
		);

		expect(screen.getByTestId("pi-globe")).toHaveClass(
			"size-4",
			"text-primary",
		);
		expect(screen.getByTestId("fa-android")).toHaveClass(
			"h-2.5",
			"w-2.5",
			"text-emerald-600",
		);
	});

	it("falls back to the inferred device icon when the operating system is unknown", () => {
		render(<SessionPlatformIcon userAgent="CustomTablet/1.0 (Tablet)" />);

		expect(screen.getByTestId("pi-globe")).toBeInTheDocument();
		expect(screen.getByTestId("fa-tablet")).toHaveClass(
			"h-2.5",
			"w-2.5",
			"text-violet-600",
		);
	});

	it("uses the generic display fallback when nothing can be inferred", () => {
		render(<SessionPlatformIcon userAgent="CustomAgent/1.0" />);

		expect(screen.getByTestId("pi-globe")).toBeInTheDocument();
		expect(screen.getByTestId("fa-display")).toHaveClass(
			"h-2.5",
			"w-2.5",
			"text-muted-foreground",
		);
	});
});
