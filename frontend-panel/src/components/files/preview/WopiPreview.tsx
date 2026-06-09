import {
	type ReactNode,
	useCallback,
	useEffect,
	useMemo,
	useRef,
	useState,
	useSyncExternalStore,
} from "react";
import { useTranslation } from "react-i18next";
import { EmptyState } from "@/components/common/EmptyState";
import { Button } from "@/components/ui/button";
import { Icon } from "@/components/ui/icon";
import type { PreviewOpenMode, WopiLaunchSession } from "@/types/api";
import {
	EmbeddedWebAppPreview,
	TRUSTED_DOCUMENT_VIEWER_IFRAME_ALLOW,
	TRUSTED_DOCUMENT_VIEWER_IFRAME_SANDBOX,
} from "./EmbeddedWebAppPreview";
import { PreviewLoadingState } from "./PreviewLoadingState";
import { PreviewSurface, PreviewSurfaceContent } from "./PreviewSurface";
import type { WopiSessionResource } from "./wopiSessionResource";

interface WopiPreviewProps {
	label: string;
	rawConfig: Record<string, unknown> | null | undefined;
	sessionResource: WopiSessionResource;
}

interface WopiSessionRequestKey {
	sessionResource: WopiSessionResource;
}

interface WopiFrameSubmission {
	key: string;
	requestKey: WopiSessionRequestKey;
}

function normalizeWopiMode(
	rawConfig: Record<string, unknown> | null | undefined,
	session: WopiLaunchSession | null,
): PreviewOpenMode {
	if (session?.mode === "new_tab") {
		return "new_tab";
	}
	if (session?.mode === "iframe") {
		return "iframe";
	}
	return rawConfig?.mode === "new_tab" ? "new_tab" : "iframe";
}

function buildWopiFormFields(
	session: WopiLaunchSession,
): Record<string, string> {
	return {
		access_token: session.access_token,
		access_token_ttl: String(session.access_token_ttl),
		...(session.form_fields ?? {}),
	};
}

function WopiPreviewStatePane({
	action,
	description,
	icon,
	title,
}: {
	action?: ReactNode;
	description: string;
	icon: ReactNode;
	title: string;
}) {
	return (
		<PreviewSurface>
			<PreviewSurfaceContent>
				<EmptyState
					icon={icon}
					title={title}
					description={description}
					action={action}
				/>
			</PreviewSurfaceContent>
		</PreviewSurface>
	);
}

export function WopiPreview({
	label,
	rawConfig,
	sessionResource,
}: WopiPreviewProps) {
	const { t } = useTranslation("files");
	const iframeNameRef = useRef(
		`wopi-preview-${Math.random().toString(36).slice(2, 10)}`,
	);
	const formRef = useRef<HTMLFormElement | null>(null);
	const submittedFrameRef = useRef<WopiFrameSubmission | null>(null);
	const [loadedFrameSubmission, setLoadedFrameSubmission] =
		useState<WopiFrameSubmission | null>(null);
	const requestKey = useMemo<WopiSessionRequestKey>(
		() => ({ sessionResource }),
		[sessionResource],
	);
	const sessionState = useSyncExternalStore(
		sessionResource.subscribe,
		sessionResource.getSnapshot,
		sessionResource.getSnapshot,
	);
	const isLoading = sessionState.loading;
	const session = isLoading ? null : sessionState.session;

	const mode = useMemo(
		() => normalizeWopiMode(rawConfig, session),
		[rawConfig, session],
	);
	const formFields = useMemo<Record<string, string>>(
		() => (session ? buildWopiFormFields(session) : {}),
		[session],
	);
	const frameSubmissionKey =
		session && mode !== "new_tab"
			? `${session.action_url}\u0000${session.access_token}\u0000${mode}`
			: null;
	const isFrameLoaded =
		submittedFrameRef.current?.requestKey === requestKey &&
		submittedFrameRef.current.key === frameSubmissionKey &&
		loadedFrameSubmission?.requestKey === requestKey &&
		loadedFrameSubmission.key === frameSubmissionKey;

	const submitToTarget = useCallback(
		(target: string) => {
			const form = formRef.current;
			if (!form || !session) {
				return false;
			}

			form.target = target;
			form.submit();
			return true;
		},
		[session],
	);

	useEffect(() => {
		if (!session || !frameSubmissionKey || mode === "new_tab") {
			submittedFrameRef.current = null;
			return;
		}

		if (
			submittedFrameRef.current?.requestKey === requestKey &&
			submittedFrameRef.current.key === frameSubmissionKey
		) {
			return;
		}
		submittedFrameRef.current = { key: frameSubmissionKey, requestKey };

		const frameTarget = iframeNameRef.current;
		const timer = window.requestAnimationFrame(() => {
			submitToTarget(frameTarget);
		});

		return () => {
			window.cancelAnimationFrame(timer);
		};
	}, [frameSubmissionKey, mode, requestKey, session, submitToTarget]);

	const openExternally = useCallback(() => {
		const target = `wopi-external-${Math.random().toString(36).slice(2, 10)}`;
		window.open("", target, "noopener,noreferrer");
		submitToTarget(target);
	}, [submitToTarget]);

	if (isLoading) {
		return (
			<PreviewLoadingState
				text={t("wopi_loading", { label })}
				className="h-full min-h-[16rem]"
			/>
		);
	}

	if (!session) {
		return (
			<WopiPreviewStatePane
				icon={<Icon name="Globe" className="size-10" />}
				title={t("wopi_unavailable")}
				description={t("wopi_unavailable_desc")}
			/>
		);
	}

	const form = (
		<form
			ref={formRef}
			action={session.action_url}
			method="post"
			className="hidden"
		>
			{Object.entries(formFields).map(([key, value]) => (
				<input key={key} type="hidden" name={key} value={value} />
			))}
		</form>
	);

	if (mode === "new_tab") {
		return (
			<>
				{form}
				<WopiPreviewStatePane
					icon={<Icon name="ArrowSquareOut" className="size-10" />}
					title={label}
					description={t("wopi_external_desc", { label })}
					action={
						<Button variant="outline" onClick={openExternally}>
							<Icon name="ArrowSquareOut" className="mr-2 size-4" />
							{t("wopi_open", { label })}
						</Button>
					}
				/>
			</>
		);
	}

	return (
		<>
			{form}
			<EmbeddedWebAppPreview
				title={label}
				src="about:blank"
				iframeName={iframeNameRef.current}
				onLoad={() => {
					if (
						!frameSubmissionKey ||
						submittedFrameRef.current?.requestKey !== requestKey ||
						submittedFrameRef.current.key !== frameSubmissionKey
					) {
						return;
					}
					setLoadedFrameSubmission({ key: frameSubmissionKey, requestKey });
				}}
				iframeHidden={!isFrameLoaded}
				iframeAllow={TRUSTED_DOCUMENT_VIEWER_IFRAME_ALLOW}
				iframeReferrerPolicy="no-referrer"
				iframeSandbox={TRUSTED_DOCUMENT_VIEWER_IFRAME_SANDBOX}
				actions={
					<Button variant="outline" size="sm" onClick={openExternally}>
						<Icon name="ArrowSquareOut" className="mr-2 size-4" />
						{t("wopi_open", { label })}
					</Button>
				}
				loadingOverlay={
					!isFrameLoaded ? (
						<PreviewLoadingState
							text={t("wopi_loading", { label })}
							className="h-full min-h-0"
						/>
					) : undefined
				}
			/>
		</>
	);
}
