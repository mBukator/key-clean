import { invoke } from "@tauri-apps/api/core";
import { getCurrentWebviewWindow } from "@tauri-apps/api/webviewWindow";
import { useEffect, useState } from "react";

import { isStringKey, t, type StringKey } from "../shared/localization/t";
import {
    STATUS_EVENT,
    type EndReason,
    type EngineStatus,
    type ErrorInfo,
    type Keyboard,
    type SessionState,
} from "../shared/types/engine";

const STATE_KEYS: Record<SessionState, StringKey> = {
    idle: "status.idle",
    starting: "status.starting",
    locked: "status.locked",
    unlocking: "status.unlocking",
};

const END_REASON_KEYS: Record<EndReason, StringKey> = {
    timeout: "endReason.timeout",
    emergency: "endReason.emergency",
    hardDeadline: "endReason.hardDeadline",
    suspend: "endReason.suspend",
    endSession: "endReason.endSession",
    sessionLock: "endReason.sessionLock",
    sessionDisconnect: "endReason.sessionDisconnect",
    engineError: "endReason.engineError",
    userRequest: "endReason.userRequest",
};

function toErrorInfo(err: unknown): ErrorInfo {
    if (typeof err === "object" && err !== null && "messageKey" in err) {
        const { messageKey, details } = err as { messageKey: unknown; details?: unknown };
        if (typeof messageKey === "string") {
            return { messageKey, details: typeof details === "string" ? details : "" };
        }
    }
    return { messageKey: "error.ipc", details: String(err) };
}

function ErrorMessage({ error }: { error: ErrorInfo }) {
    const message = isStringKey(error.messageKey)
        ? t(error.messageKey)
        : t("error.unknown");
    return (
        <div
            role="alert"
            className="rounded border border-red-300 bg-red-50 p-3 text-red-900"
        >
            <p>{message}</p>
            {error.details && (
                <details className="mt-2 text-sm">
                    <summary className="cursor-pointer">{t("error.details")}</summary>
                    <pre className="mt-1 whitespace-pre-wrap break-words">
                        {error.details}
                    </pre>
                </details>
            )}
        </div>
    );
}

export function App() {
    const [status, setStatus] = useState<EngineStatus | null>(null);
    const [keyboards, setKeyboards] = useState<Keyboard[] | null>(null);
    const [requestError, setRequestError] = useState<ErrorInfo | null>(null);

    useEffect(() => {
        let unlisten: (() => void) | undefined;
        let disposed = false;

        getCurrentWebviewWindow()
            .listen<EngineStatus>(STATUS_EVENT, (event) => {
                setStatus(event.payload);
            })
            .then((fn) => {
                if (disposed) {
                    fn();
                } else {
                    unlisten = fn;
                }
            })
            .catch((err: unknown) => {
                setRequestError(toErrorInfo(err));
            });

        invoke<EngineStatus>("get_status")
            .then(setStatus)
            .catch((err: unknown) => {
                setRequestError(toErrorInfo(err));
            });

        invoke<Keyboard[]>("list_keyboards")
            .then(setKeyboards)
            .catch((err: unknown) => {
                setRequestError(toErrorInfo(err));
            });

        return () => {
            disposed = true;
            unlisten?.();
        };
    }, []);

    const lock = () => {
        setRequestError(null);
        invoke("lock_keyboard").catch((err: unknown) => {
            setRequestError(toErrorInfo(err));
        });
    };

    const state = status?.state ?? "idle";
    const canLock = status?.engineAvailable === true && state === "idle";
    const error = requestError ?? status?.error ?? null;

    return (
        <main className="flex min-h-screen flex-col gap-6 p-6">
            <header className="flex flex-wrap items-center gap-3">
                <h1 className="text-2xl font-semibold">{t("app.title")}</h1>
                {status?.devCap && (
                    <span
                        title={t("dev.capHint")}
                        className="rounded bg-amber-200 px-2 py-0.5 text-xs font-bold text-amber-900"
                    >
                        {t("dev.cap")}
                    </span>
                )}
            </header>

            <section className="flex flex-col gap-3">
                <button
                    type="button"
                    onClick={lock}
                    disabled={!canLock}
                    className="self-start rounded bg-neutral-900 px-5 py-3 text-white disabled:opacity-40"
                >
                    {t("lock.button", { seconds: status?.lockSeconds ?? "…" })}
                </button>
                <p className="text-sm text-neutral-600">{t("lock.hint")}</p>
                <p aria-live="polite" className="font-medium">
                    {t(STATE_KEYS[state])}
                </p>
                {status?.lastEndReason && state === "idle" && (
                    <p className="text-sm text-neutral-600">
                        {t("status.lastEnd", {
                            reason: t(END_REASON_KEYS[status.lastEndReason]),
                        })}
                    </p>
                )}
                {error && <ErrorMessage error={error} />}
            </section>

            <section className="flex flex-col gap-2">
                <h2 className="text-lg font-semibold">{t("devices.title")}</h2>
                {keyboards === null ? null : keyboards.length === 0 ? (
                    <p className="text-sm text-neutral-600">{t("devices.none")}</p>
                ) : (
                    <ul className="flex flex-col gap-1">
                        {keyboards.map((k) => (
                            <li key={k.id} className="break-words">
                                {k.name ?? t("devices.unnamed")}
                            </li>
                        ))}
                    </ul>
                )}
                <p className="text-xs text-neutral-500">{t("devices.allLocked")}</p>
            </section>
        </main>
    );
}
