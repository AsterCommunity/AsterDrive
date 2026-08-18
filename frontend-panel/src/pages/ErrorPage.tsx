import { useTranslation } from "react-i18next";
import {
	isRouteErrorResponse,
	useLocation,
	useNavigate,
	useRouteError,
} from "react-router-dom";
import { Badge } from "@/components/ui/badge";
import { Button } from "@/components/ui/button";
import { Icon } from "@/components/ui/icon";
import { runtimeFlags } from "@/config/runtime";
import { usePageTitle } from "@/hooks/usePageTitle";

type ErrorTone = {
	label: string;
	title: string;
	description: string;
	suggestion: string;
};

type DeveloperDetails = {
	name: string | null;
	payload: string | null;
	stack: string | null;
};

type Translate = (key: string, options?: Record<string, unknown>) => string;

function getErrorTones(t: Translate): {
	defaultTone: ErrorTone;
	statusTones: Record<number, ErrorTone>;
} {
	return {
		defaultTone: {
			label: t("error_page_default_label"),
			title: t("error_page_default_title"),
			description: t("error_page_default_description"),
			suggestion: t("error_page_default_suggestion"),
		},
		statusTones: {
			401: {
				label: t("error_page_sign_in_required_label"),
				title: t("error_page_sign_in_required_title"),
				description: t("error_page_sign_in_required_description"),
				suggestion: t("error_page_sign_in_required_suggestion"),
			},
			403: {
				label: t("error_page_access_blocked_label"),
				title: t("error_page_access_blocked_title"),
				description: t("error_page_access_blocked_description"),
				suggestion: t("error_page_access_blocked_suggestion"),
			},
			404: {
				label: t("error_page_route_missing_label"),
				title: t("error_page_route_missing_title"),
				description: t("error_page_route_missing_description"),
				suggestion: t("error_page_route_missing_suggestion"),
			},
			500: {
				label: t("error_page_server_fault_label"),
				title: t("error_page_server_fault_title"),
				description: t("error_page_server_fault_description"),
				suggestion: t("error_page_server_fault_suggestion"),
			},
		},
	};
}

function getErrorContent(
	error: unknown,
	t: Translate,
	defaultTone: ErrorTone,
	statusTones: Record<number, ErrorTone>,
) {
	let status: number | null = null;
	let message = t("error_page_default_message");

	if (isRouteErrorResponse(error)) {
		status = error.status;
		message = error.statusText || message;
	} else if (error instanceof Error) {
		message = error.message;
	} else if (error == null) {
		status = 404;
		message = t("error_page_missing_route_message");
	}

	return {
		status,
		message,
		tone: status ? (statusTones[status] ?? defaultTone) : defaultTone,
	};
}

function serializeDeveloperValue(value: unknown) {
	if (value == null) return null;
	if (typeof value === "string") return value;

	try {
		return JSON.stringify(value, null, 2);
	} catch {
		return String(value);
	}
}

function getDeveloperDetails(error: unknown): DeveloperDetails {
	if (isRouteErrorResponse(error)) {
		return {
			name: "RouteErrorResponse",
			payload: serializeDeveloperValue(error.data),
			stack: null,
		};
	}

	if (error instanceof Error) {
		return {
			name: error.name || null,
			payload: null,
			stack: error.stack ?? null,
		};
	}

	if (error == null) {
		return {
			name: null,
			payload: null,
			stack: null,
		};
	}

	return {
		name: typeof error,
		payload: serializeDeveloperValue(error),
		stack: null,
	};
}

export default function ErrorPage() {
	const { t } = useTranslation("errors");
	const error = useRouteError();
	const navigate = useNavigate();
	const location = useLocation();
	const { defaultTone, statusTones } = getErrorTones(t);
	const { status, message, tone } = getErrorContent(
		error,
		t,
		defaultTone,
		statusTones,
	);
	usePageTitle(tone.title);
	const developerDetails = getDeveloperDetails(error);
	const isDeveloperMode = runtimeFlags.showDeveloperErrorDetails;

	const routeLabel =
		location.pathname === "/"
			? t("error_page_root_workspace")
			: location.pathname;
	const statusDisplay = status ? String(status) : "ERR";
	const responseLabel =
		status !== null
			? t("error_page_response_http", { status })
			: t("error_page_runtime_exception");
	const badgeVariant: "destructive" | "outline" =
		status !== null && status >= 500 ? "destructive" : "outline";

	const handleGoBack = () => {
		if (typeof window !== "undefined" && window.history.length > 1) {
			navigate(-1);
			return;
		}

		navigate("/");
	};

	return (
		<main className="flex min-h-screen bg-background text-foreground">
			<div className="m-auto w-full max-w-4xl space-y-8 px-6 py-10">
				<header>
					<div className="flex flex-col gap-5 md:flex-row md:items-start md:justify-between">
						<div className="flex items-start gap-4">
							<div className="flex size-14 shrink-0 items-center justify-center rounded-2xl bg-muted/40">
								<Icon
									name="CircleAlert"
									className="size-7 text-muted-foreground"
								/>
							</div>

							<div className="min-w-0 space-y-3">
								<div className="flex flex-wrap items-center gap-2">
									<Badge variant={badgeVariant} className="font-medium">
										{tone.label}
									</Badge>
									{isDeveloperMode ? (
										<span className="text-sm text-muted-foreground">
											{responseLabel}
										</span>
									) : null}
								</div>

								<div className="space-y-1">
									<h1 className="text-2xl font-semibold tracking-tight sm:text-3xl">
										{tone.title}
									</h1>
									<p className="max-w-2xl text-sm leading-6 text-muted-foreground sm:text-base">
										{tone.description}
									</p>
								</div>
							</div>
						</div>

						{isDeveloperMode ? (
							<div className="w-full rounded-xl bg-muted/30 px-4 py-3 md:w-auto md:min-w-28 md:text-right">
								<p className="text-xs font-medium uppercase tracking-[0.18em] text-muted-foreground">
									{t("error_page_status_heading")}
								</p>
								<p className="mt-1 text-3xl font-semibold tracking-tight">
									{statusDisplay}
								</p>
							</div>
						) : null}
					</div>
				</header>

				{isDeveloperMode ? (
					<section className="grid gap-4 border-t pt-8 md:grid-cols-[minmax(0,1fr)_280px]">
						<div className="space-y-4">
							<div className="rounded-xl bg-muted/30 p-4">
								<p className="text-sm font-medium">
									{t("error_page_developer_detail")}
								</p>

								<div className="mt-3 space-y-3">
									{developerDetails.name ? (
										<div>
											<p className="text-xs font-medium uppercase tracking-[0.18em] text-muted-foreground">
												{t("error_page_error_name")}
											</p>
											<p className="mt-2 font-mono text-sm">
												{developerDetails.name}
											</p>
										</div>
									) : null}

									<div>
										<p className="text-xs font-medium uppercase tracking-[0.18em] text-muted-foreground">
											{t("error_page_message")}
										</p>
										<p className="mt-2 break-all font-mono text-sm">
											{message}
										</p>
									</div>

									{developerDetails.payload &&
									developerDetails.payload !== message ? (
										<div>
											<p className="text-xs font-medium uppercase tracking-[0.18em] text-muted-foreground">
												{t("error_page_payload")}
											</p>
											<pre className="mt-2 overflow-x-auto whitespace-pre-wrap break-all rounded-lg bg-background p-3 font-mono text-xs">
												{developerDetails.payload}
											</pre>
										</div>
									) : null}

									{developerDetails.stack ? (
										<div>
											<p className="text-xs font-medium uppercase tracking-[0.18em] text-muted-foreground">
												{t("error_page_stack")}
											</p>
											<pre className="mt-2 max-h-56 overflow-auto whitespace-pre-wrap break-all rounded-lg bg-background p-3 font-mono text-xs">
												{developerDetails.stack}
											</pre>
										</div>
									) : null}
								</div>
							</div>

							<div className="grid gap-3 border-t pt-4 sm:grid-cols-2">
								<div>
									<p className="text-xs font-medium uppercase tracking-[0.18em] text-muted-foreground">
										{t("error_page_path")}
									</p>
									<p className="mt-2 break-all font-mono text-sm">
										{routeLabel}
									</p>
								</div>
								<div>
									<p className="text-xs font-medium uppercase tracking-[0.18em] text-muted-foreground">
										{t("error_page_response")}
									</p>
									<p className="mt-2 font-mono text-sm">{responseLabel}</p>
								</div>
							</div>
						</div>

						<div className="h-fit rounded-xl bg-muted/30 p-4">
							<p className="text-sm font-medium">
								{t("error_page_recovery_title")}
							</p>
							<p className="mt-2 text-sm leading-6 text-muted-foreground">
								{tone.suggestion}
							</p>
						</div>
					</section>
				) : null}

				<footer className="flex justify-end border-t pt-8">
					<div className="flex flex-col gap-3 sm:flex-row">
						<Button variant="outline" onClick={handleGoBack}>
							<Icon name="Undo" className="mr-2 size-4" />
							{t("error_page_go_back")}
						</Button>
						<Button onClick={() => navigate("/")}>
							<Icon name="House" className="mr-2 size-4" />
							{t("error_page_go_home")}
						</Button>
					</div>
				</footer>
			</div>
		</main>
	);
}
