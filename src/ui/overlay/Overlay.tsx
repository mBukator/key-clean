import { invoke } from "@tauri-apps/api/core";
import { getCurrentWebviewWindow } from "@tauri-apps/api/webviewWindow";
import { useEffect, useRef, useState } from "react";

import { isStringKey, t, type StringKey } from "../../shared/localization/t";
import {
    STATUS_EVENT,
    type EngineStatus,
    type LockTargets,
    type OverlaySession,
} from "../../shared/types/engine";
import { EmergencyShortcut } from "./EmergencyShortcut";
import { MONO_FONT } from "./font";
import { SegmentedProgress } from "./SegmentedProgress";

/** The countdown turns to the accent colour for this many final seconds. */
const LAST_SECONDS = 5;

const UNLOCKING_IN_KEYS = {
    one: "overlay.unlockingIn.one",
    other: "overlay.unlockingIn.other",
} satisfies Record<"one" | "other", StringKey>;

const plurals = new Intl.PluralRules("en");

/** Invoke errors are ignored here: the app enforces every deadline itself. */
function ignore(): void {
    // Nothing to do.
}

function devicesKey(targets: LockTargets): StringKey {
    if (targets.mouse) {
        return targets.keyboard ? "overlay.devices.all" : "overlay.devices.mouse";
    }
    return "overlay.devices.keyboard";
}

/** "01:42". */
function formatCountdown(seconds: number): string {
    const minutes = Math.floor(seconds / 60);
    const rest = seconds % 60;
    return `${String(minutes).padStart(2, "0")}:${String(rest).padStart(2, "0")}`;
}

/** The full-screen lock screen (invariant 11). It shows the lock; the engine owns it. */
export function Overlay() {
    // undefined while loading, null when there is no session to show.
    const [session, setSession] = useState<OverlaySession | null | undefined>(undefined);
    const [status, setStatus] = useState<EngineStatus | null>(null);
    // The last countdown the engine sent, kept once the lock ends so the closing frames don't jump
    // back to the full duration.
    const [lastCountdown, setLastCountdown] = useState<number | null>(null);
    const attemptRef = useRef<number | null>(null);
    const confirmedRef = useRef(false);

    // Subscribe first, then ask, so no status in between is missed.
    useEffect(() => {
        let unlisten: (() => void) | undefined;
        let disposed = false;

        getCurrentWebviewWindow()
            .listen<EngineStatus>(STATUS_EVENT, (event) => {
                setStatus(event.payload);
                const countdown = event.payload.countdownSecs;
                if (countdown !== null) {
                    setLastCountdown(countdown);
                }
                const attempt = attemptRef.current;
                if (attempt !== null) {
                    invoke("overlay_tick", { attempt }).then(ignore, ignore);
                }
            })
            .then((fn) => {
                if (disposed) {
                    fn();
                    return;
                }
                unlisten = fn;
                invoke<OverlaySession | null>("get_overlay_session").then((current) => {
                    if (disposed) {
                        return;
                    }
                    attemptRef.current = current?.attempt ?? null;
                    setSession(current);
                }, ignore);
                invoke<EngineStatus>("get_status").then((current) => {
                    if (disposed) {
                        return;
                    }
                    // An event that arrived meanwhile is newer than this answer.
                    setStatus((latest) => latest ?? current);
                }, ignore);
            }, ignore);

        return () => {
            disposed = true;
            unlisten?.();
        };
    }, []);

    const attempt = session?.attempt ?? null;

    // Runs after the locked layout has committed: two frames later it has been painted, so the
    // page confirms it is visible, once. The app enforces the time budget.
    useEffect(() => {
        if (attempt === null || confirmedRef.current) {
            return undefined;
        }
        let cancelled = false;
        let frame = 0;

        const confirm = () => {
            if (cancelled || confirmedRef.current) {
                return;
            }
            confirmedRef.current = true;
            invoke("overlay_ready", { attempt }).then(ignore, ignore);
        };
        const onVisibilityChange = () => {
            if (document.visibilityState === "visible") {
                document.removeEventListener("visibilitychange", onVisibilityChange);
                confirm();
            }
        };

        frame = requestAnimationFrame(() => {
            frame = requestAnimationFrame(() => {
                frame = 0;
                if (document.visibilityState === "visible") {
                    confirm();
                } else {
                    document.addEventListener("visibilitychange", onVisibilityChange);
                }
            });
        });

        return () => {
            cancelled = true;
            if (frame !== 0) {
                cancelAnimationFrame(frame);
            }
            document.removeEventListener("visibilitychange", onVisibilityChange);
        };
    }, [attempt]);

    if (!session) {
        return null;
    }

    const active = status?.state === "starting" || status?.state === "locked";
    const targets: LockTargets =
        active && status.targets
            ? status.targets
            : { keyboard: session.keyboard, mouse: session.mouse };
    const remaining = status?.countdownSecs ?? lastCountdown ?? session.seconds;
    const total = session.seconds;
    const lastSeconds = remaining >= 1 && remaining <= LAST_SECONDS;
    const devCap = status?.devCap === true || session.devCap;
    // Before the lock starts, the status still carries the previous session's notice.
    const notice =
        active && status.notice && isStringKey(status.notice) ? t(status.notice) : null;
    const accentText = lastSeconds ? "text-[#f5a54a]" : "text-[#f2efe9]";

    const unlock = () => {
        invoke("unlock_input").then(ignore, ignore);
    };

    return (
        <main className="flex min-h-dvh flex-col justify-between gap-8 bg-[#0a0908] p-[clamp(1.5rem,4vw,3.5rem)] text-[#f2efe9] select-none">
            <header className="flex flex-wrap items-start justify-between gap-4">
                <div className="flex min-w-0 flex-col gap-1">
                    <h1 className="text-base font-bold break-words">{t("app.title")}</h1>
                    <p className="text-sm break-words text-[#a19d96]">
                        {t("overlay.inputLocked", { devices: t(devicesKey(targets)) })}
                    </p>
                    {targets.mouse && (
                        <p className="text-sm break-words text-[#a19d96]">
                            {t("overlay.mouseHint")}
                        </p>
                    )}
                    {notice && (
                        <p
                            aria-live="polite"
                            className="text-sm break-words text-[#d9cfc1]"
                        >
                            {notice}
                        </p>
                    )}
                </div>
                <div className="flex flex-wrap items-center justify-end gap-3">
                    {devCap && (
                        <span
                            title={t("dev.capHint")}
                            className="rounded border border-[#f5a54a]/60 px-2 py-0.5 text-xs font-bold text-[#f5a54a]"
                        >
                            {t("dev.cap")}
                        </span>
                    )}
                    {!targets.mouse && (
                        <button
                            type="button"
                            onClick={unlock}
                            className="rounded-md border border-[#4a4540] px-4 py-2 text-sm font-semibold hover:bg-white/5 focus-visible:outline-2 focus-visible:outline-offset-2 focus-visible:outline-[#f2efe9]"
                        >
                            {t("lock.unlockNow")}
                        </button>
                    )}
                </div>
            </header>

            <footer className="flex flex-col gap-6">
                <div className="flex flex-wrap items-end justify-between gap-x-10 gap-y-6">
                    <div className="flex min-w-0 flex-col gap-2">
                        {lastSeconds && (
                            <p
                                aria-live="polite"
                                className="text-lg font-semibold break-words text-[#f5a54a]"
                            >
                                {t(
                                    UNLOCKING_IN_KEYS[
                                        plurals.select(remaining) === "one"
                                            ? "one"
                                            : "other"
                                    ],
                                    { count: remaining }
                                )}
                            </p>
                        )}
                        <span
                            role="timer"
                            style={{
                                ...MONO_FONT,
                                fontSize: "clamp(4rem, min(19vw, 36vh), 30rem)",
                            }}
                            className={`leading-none font-light tabular-nums ${accentText}`}
                        >
                            {formatCountdown(remaining)}
                        </span>
                    </div>
                    <EmergencyShortcut />
                </div>
                <SegmentedProgress
                    remaining={remaining}
                    total={total}
                    accent={lastSeconds}
                />
            </footer>
        </main>
    );
}
