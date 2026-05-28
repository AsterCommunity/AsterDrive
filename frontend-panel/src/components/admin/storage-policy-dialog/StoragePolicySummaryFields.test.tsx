import { render, screen } from "@testing-library/react";
import { describe, expect, it, vi } from "vitest";
import type { StoragePolicyCapacityInfo } from "@/types/api";
import {
	PolicyCapacityCard,
	PolicySummaryCard,
	StorageDriverVisual,
} from "./StoragePolicySummaryFields";

vi.mock("@/components/ui/badge", () => ({
	Badge: ({
		children,
		className,
		"data-testid": dataTestId,
		variant,
	}: {
		children: React.ReactNode;
		className?: string;
		"data-testid"?: string;
		variant?: string;
	}) => (
		<span className={className} data-testid={dataTestId} data-variant={variant}>
			{children}
		</span>
	),
}));

vi.mock("@/components/ui/icon", () => ({
	Icon: ({ className, name }: { className?: string; name: string }) => (
		<span className={className}>{`icon:${name}`}</span>
	),
}));

vi.mock("@/lib/format", () => ({
	formatBytes: (value: number) => `bytes:${value}`,
}));

function t(key: string, values?: Record<string, unknown>) {
	if (key === "policy_capacity_total") return `total:${values?.total}`;
	if (key === "policy_capacity_blob_count") return `count:${values?.count}`;
	return key;
}

function createCapacity(
	overrides: Partial<StoragePolicyCapacityInfo> = {},
): StoragePolicyCapacityInfo {
	return {
		blob_count: 3,
		blob_total_bytes: 300,
		capacity: {
			available_bytes: 600,
			observed_at: "2026-05-01T00:00:00Z",
			source: "local_filesystem",
			status: "supported",
			total_bytes: 1000,
			used_bytes: 400,
		},
		driver_type: "local",
		policy_id: 1,
		...overrides,
	};
}

describe("StoragePolicySummaryFields", () => {
	it("renders driver visuals from icon sources and fallback icon names", () => {
		const { container } = render(
			<>
				<StorageDriverVisual
					option={{
						iconName: "HardDrive",
						labelKey: "driver_type_local",
						type: "local",
					}}
				/>
				<StorageDriverVisual
					option={{
						iconSrc: "/s3.svg",
						labelKey: "driver_type_s3",
						type: "s3",
					}}
				/>
			</>,
		);

		expect(screen.getByText("icon:HardDrive")).toBeInTheDocument();
		expect(container.querySelector("img")).toHaveAttribute("src", "/s3.svg");
	});

	it("renders summary card labels with a new-policy fallback name", () => {
		render(
			<PolicySummaryCard
				currentStorageOption={{
					iconName: "HardDrive",
					labelKey: "driver_type_local",
					type: "local",
				}}
				description="Local policy description"
				formName=""
				items={[
					{ label: "driver", value: "Local" },
					{ label: "path", value: "/srv/data" },
				]}
				t={t}
			/>,
		);

		expect(screen.getByText("policy_wizard_summary_title")).toBeInTheDocument();
		expect(screen.getByText("new_policy")).toBeInTheDocument();
		expect(screen.getByText("Local policy description")).toBeInTheDocument();
		expect(screen.getByText("driver")).toBeInTheDocument();
		expect(screen.getByText("/srv/data")).toBeInTheDocument();
	});

	it("renders supported capacity segments and blob metadata", () => {
		render(
			<PolicyCapacityCard capacity={createCapacity()} loading={false} t={t} />,
		);

		expect(
			screen.getByText("policy_capacity_status_supported"),
		).toBeInTheDocument();
		expect(screen.getByText("bytes:300")).toBeInTheDocument();
		expect(screen.getByText("count:3")).toBeInTheDocument();
		expect(screen.getByText("bytes:400")).toBeInTheDocument();
		expect(screen.getByText("bytes:600")).toBeInTheDocument();
		expect(screen.getByText("total:bytes:1000")).toBeInTheDocument();
		expect(
			screen.getByText("policy_capacity_other_system_used"),
		).toBeInTheDocument();
	});

	it("renders loading, unsupported, and unavailable capacity descriptions", () => {
		const { rerender } = render(
			<PolicyCapacityCard capacity={null} loading={true} t={t} />,
		);
		expect(screen.getByText("policy_capacity_checking")).toBeInTheDocument();
		expect(screen.getByText("policy_capacity_loading")).toBeInTheDocument();

		rerender(
			<PolicyCapacityCard
				capacity={createCapacity({
					blob_total_bytes: undefined,
					capacity: {
						available_bytes: null,
						observed_at: "2026-05-01T00:00:00Z",
						source: "s3_head_bucket",
						status: "unsupported",
						total_bytes: null,
						used_bytes: null,
					},
				})}
				loading={false}
				t={t}
			/>,
		);
		expect(
			screen.getByText("policy_capacity_unsupported_desc"),
		).toBeInTheDocument();

		rerender(<PolicyCapacityCard capacity={null} loading={false} t={t} />);
		expect(
			screen.getByText("policy_capacity_unavailable_desc"),
		).toBeInTheDocument();
	});
});
