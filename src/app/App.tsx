import { invoke } from "@tauri-apps/api/core";
import { getCurrentWebviewWindow } from "@tauri-apps/api/webviewWindow";
import { useEffect, useState } from "react";

import { isStringKey, t, type StringKey } from "../shared/localization/t";
import {
    DEVICES_EVENT,
    STATUS_EVENT,
    type DevicesState,
    type EndReason,
    type EngineStatus,
    type ErrorInfo,
    type LockOptions,
    type LockTargets,
    type SessionState,
} from "../shared/types/engine";
import { DeviceList } from "../ui/devices/DeviceList";

const STATE_KEYS: Record<Exclude<SessionState, "locked">, StringKey> = {
    idle: "status.idle",
    starting: "status.starting",
    unlocking: "status.unlocking",
};

/** The status line for a lock, from what the engine says it blocks. */
function lockedKey(targets: LockTargets | null): StringKey {
    if (targets?.mouse) {
        return targets.keyboard ? "status.locked.all" : "status.locked.mouse";
    }
    return "status.locked.keyboard";
}

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
    overlayLost: "endReason.overlayLost",
};

const DURATION_KEYS = {
    seconds: { one: "duration.seconds.one", other: "duration.seconds.other" },
    minutes: { one: "duration.minutes.one", other: "duration.minutes.other" },
} satisfies Record<string, Record<"one" | "other", StringKey>>;

const plurals = new Intl.PluralRules("en");

/** "30 seconds", "1 minute", "5 minutes". */
function durationLabel(seconds: number): string {
    const unit = seconds % 60 === 0 ? "minutes" : "seconds";
    const count = unit === "minutes" ? seconds / 60 : seconds;
    const form = plurals.select(count) === "one" ? "one" : "other";
    return t(DURATION_KEYS[unit][form], { count });
}

/** "02:00" (§15). */
function formatCountdown(seconds: number): string {
    const minutes = Math.floor(seconds / 60);
    const rest = seconds % 60;
    return `${String(minutes).padStart(2, "0")}:${String(rest).padStart(2, "0")}`;
}

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
    const [options, setOptions] = useState<LockOptions | null>(null);
    const [selected, setSelected] = useState<number | null>(null);
    const [devices, setDevices] = useState<DevicesState | null>(null);
    const [requestError, setRequestError] = useState<ErrorInfo | null>(null);
    const [lockKeyboard, setLockKeyboard] = useState(true);
    const [lockMouse, setLockMouse] = useState(false);

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

        invoke<LockOptions>("get_lock_options")
            .then(setOptions)
            .catch((err: unknown) => {
                setRequestError(toErrorInfo(err));
            });

        return () => {
            disposed = true;
            unlisten?.();
        };
    }, []);

    // The device list updates when devices are connected or disconnected. Subscribes first, then
    // asks for the current list, so an update in between isn't missed.
    useEffect(() => {
        let unlisten: (() => void) | undefined;
        let disposed = false;

        getCurrentWebviewWindow()
            .listen<DevicesState>(DEVICES_EVENT, (event) => {
                setDevices(event.payload);
            })
            .then((fn) => {
                if (disposed) {
                    fn();
                    return undefined;
                }
                unlisten = fn;
                return invoke<DevicesState>("list_devices").then((current) => {
                    // An event that arrived meanwhile is newer than this answer.
                    setDevices((latest) => latest ?? current);
                });
            })
            .catch((err: unknown) => {
                setRequestError(toErrorInfo(err));
            });

        return () => {
            disposed = true;
            unlisten?.();
        };
    }, []);

    const seconds = selected ?? options?.defaultSeconds ?? null;

    const lock = () => {
        if (seconds === null) {
            return;
        }
        setRequestError(null);
        invoke("lock_input", { seconds, keyboard: lockKeyboard, mouse: lockMouse }).catch(
            (err: unknown) => {
                setRequestError(toErrorInfo(err));
            }
        );
    };

    const unlock = () => {
        setRequestError(null);
        invoke("unlock_input").catch((err: unknown) => {
            setRequestError(toErrorInfo(err));
        });
    };

    const state = status?.state ?? "idle";
    const idle = state === "idle";
    const hasTarget = lockKeyboard || lockMouse;
    const canLock =
        status?.engineAvailable === true && idle && seconds !== null && hasTarget;
    const stateKey =
        state === "locked" ? lockedKey(status?.targets ?? null) : STATE_KEYS[state];
    const error = requestError ?? status?.error ?? null;
    const countdown = status?.countdownSecs ?? null;
    const notice = status?.notice && isStringKey(status.notice) ? t(status.notice) : null;

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
                {options && (
                    <fieldset disabled={!idle} className="flex flex-col gap-2">
                        <legend className="mb-1 font-medium">
                            {t("lock.durationLabel")}
                        </legend>
                        <div className="flex flex-wrap gap-x-4 gap-y-2">
                            {options.presetSeconds.map((preset) => (
                                <label key={preset} className="flex items-center gap-2">
                                    <input
                                        type="radio"
                                        name="duration"
                                        value={preset}
                                        checked={seconds === preset}
                                        onChange={() => {
                                            setSelected(preset);
                                        }}
                                    />
                                    {durationLabel(preset)}
                                </label>
                            ))}
                        </div>
                    </fieldset>
                )}
                <fieldset disabled={!idle} className="flex flex-col gap-2">
                    <legend className="mb-1 font-medium">{t("lock.targetsLabel")}</legend>
                    <div className="flex flex-wrap gap-x-4 gap-y-2">
                        <label className="flex items-center gap-2">
                            <input
                                type="checkbox"
                                checked={lockKeyboard}
                                onChange={(event) => {
                                    setLockKeyboard(event.target.checked);
                                }}
                            />
                            {t("lock.target.keyboard")}
                        </label>
                        <label className="flex items-center gap-2">
                            <input
                                type="checkbox"
                                checked={lockMouse}
                                onChange={(event) => {
                                    setLockMouse(event.target.checked);
                                }}
                            />
                            {t("lock.target.mouse")}
                        </label>
                    </div>
                    {lockMouse && (
                        <p className="text-sm break-words text-neutral-600">
                            {t("lock.target.mouseHint")}
                        </p>
                    )}
                </fieldset>
                <div className="flex flex-wrap items-center gap-3">
                    <button
                        type="button"
                        onClick={lock}
                        disabled={!canLock}
                        className="rounded bg-neutral-900 px-5 py-3 text-white disabled:opacity-40"
                    >
                        {t("lock.button")}
                    </button>
                    {state === "locked" && (
                        <button
                            type="button"
                            onClick={unlock}
                            className="rounded border border-neutral-900 px-5 py-3"
                        >
                            {t("lock.unlockNow")}
                        </button>
                    )}
                </div>
                <p className="text-sm text-neutral-600">{t("lock.hint")}</p>
                {countdown !== null && (
                    <p className="flex flex-wrap items-baseline gap-2">
                        <span className="text-sm text-neutral-600">
                            {t("countdown.label")}
                        </span>
                        <span
                            role="timer"
                            className="font-mono text-4xl font-semibold tabular-nums"
                        >
                            {formatCountdown(countdown)}
                        </span>
                    </p>
                )}
                <p aria-live="polite" className="font-medium">
                    {t(stateKey)}
                </p>
                {status?.lastEndReason && idle && (
                    <p aria-live="polite" className="text-sm text-neutral-600">
                        {status.lastEndReason === "timeout"
                            ? t("status.complete")
                            : t("status.lastEnd", {
                                  reason: t(END_REASON_KEYS[status.lastEndReason]),
                              })}
                    </p>
                )}
                {notice && (
                    <p
                        aria-live="polite"
                        className="rounded border border-amber-300 bg-amber-50 p-3 break-words text-amber-900"
                    >
                        {notice}
                    </p>
                )}
                {error && <ErrorMessage error={error} />}
            </section>

            <section className="flex flex-col gap-2">
                <h2 className="text-lg font-semibold">{t("devices.title")}</h2>
                {devices?.error && <ErrorMessage error={devices.error} />}
                {devices && <DeviceList devices={devices.devices} />}
                <p className="text-xs text-neutral-500">{t("devices.lockScope")}</p>
            </section>
        </main>
    );
}
