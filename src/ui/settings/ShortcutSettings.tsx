import { invoke } from "@tauri-apps/api/core";
import { useEffect, useId, useRef, useState, type KeyboardEvent } from "react";

import { t } from "../../shared/localization/t";
import type { ErrorInfo, ShortcutAction, ShortcutInfo } from "../../shared/types/engine";
import { ErrorBlock, toErrorInfo } from "../../shared/ui/ErrorBlock";
import { MONO_FONT } from "../../shared/ui/font";
import { shortcutLabel } from "../../shared/ui/shortcutKeys";

const MODIFIER_KEYS = new Set(["Control", "Shift", "Alt", "Meta", "AltGraph", "OS"]);

const EMERGENCY_KEYS = ["overlay.key.ctrl", "overlay.key.alt", "overlay.key.k"] as const;

const BUTTON =
    "rounded-md border-2 border-line bg-surface px-3 py-1.5 font-semibold focus-visible:outline-2 focus-visible:outline-offset-2 focus-visible:outline-focus disabled:cursor-not-allowed disabled:opacity-45";

/**
 * The key's name in the shortcut text Rust reads. Letters come from the layout (`event.key`), as
 * Windows registers them, so on AZERTY the key labelled A gives `A`; a layout without Latin
 * letters falls back to the key's position (`KeyK` → `K`). Digits and F-keys come from the
 * position (`Digit1` → `1`, `F5`). Anything else is passed as it is, and Rust explains why it
 * can't be used.
 */
function keyName(event: KeyboardEvent): string {
    if (/^[a-z]$/i.test(event.key)) {
        return event.key.toUpperCase();
    }
    const code = event.code;
    const letter = /^Key([A-Z])$/.exec(code);
    if (letter) {
        return letter[1] ?? code;
    }
    const digit = /^Digit(\d)$/.exec(code);
    if (digit) {
        return digit[1] ?? code;
    }
    return code;
}

/** `Ctrl+Shift+K` from a key press. The Windows key is named so Rust can refuse it. */
function shortcutText(event: KeyboardEvent): string {
    const parts: string[] = [];
    if (event.ctrlKey) parts.push("Ctrl");
    if (event.altKey) parts.push("Alt");
    if (event.shiftKey) parts.push("Shift");
    if (event.metaKey) parts.push("Win");
    parts.push(keyName(event));
    return parts.join("+");
}

function Keys({ label }: { label: string }) {
    return (
        <span style={MONO_FONT} className="font-semibold">
            {label}
        </span>
    );
}

/**
 * Settings → Shortcuts (§28, §32, ADR 0017). Picking a new shortcut unregisters the current one
 * until the pick ends, so pressing it reaches this page instead of starting a lock. The page reads
 * key presses only while picking, and keeps only the chord it sends.
 */
export function ShortcutSettings({ shortcut }: { shortcut: ShortcutInfo }) {
    const hintId = useId();
    const captureHintId = useId();
    const actionName = useId();
    const captureRef = useRef<HTMLDivElement>(null);
    const [error, setError] = useState<ErrorInfo | null>(null);
    const [saved, setSaved] = useState(false);
    const [busy, setBusy] = useState(false);

    const capturing = shortcut.state === "capturing";

    useEffect(() => {
        if (capturing) {
            captureRef.current?.focus();
        }
    }, [capturing]);

    const run = (command: string, args?: Record<string, unknown>) => {
        setBusy(true);
        setError(null);
        setSaved(false);
        return invoke<ShortcutInfo>(command, args).then(
            () => {
                setBusy(false);
                return true;
            },
            (err: unknown) => {
                setBusy(false);
                setError(toErrorInfo(err));
                return false;
            }
        );
    };

    const begin = () => {
        void run("begin_shortcut_capture");
    };

    const cancel = () => {
        if (capturing) {
            void run("end_shortcut_capture");
        }
    };

    const onKeyDown = (event: KeyboardEvent<HTMLDivElement>) => {
        // Nothing typed here reaches the page or the browser; Tab still moves focus.
        if (event.key === "Tab" && !event.ctrlKey && !event.altKey && !event.metaKey) {
            return;
        }
        event.preventDefault();
        if (busy || event.repeat || MODIFIER_KEYS.has(event.key)) {
            return;
        }
        const plain =
            !event.ctrlKey && !event.altKey && !event.shiftKey && !event.metaKey;
        if (event.key === "Escape" && plain) {
            cancel();
            return;
        }
        void run("set_shortcut", { shortcut: shortcutText(event) }).then((ok) => {
            setSaved(ok);
        });
    };

    const setAction = (action: ShortcutAction) => {
        void run("set_shortcut_action", { action });
    };

    return (
        <section className="flex flex-col gap-3">
            <h2
                style={MONO_FONT}
                className="text-xs tracking-[0.12em] text-muted uppercase"
            >
                {t("settings.shortcuts.title")}
            </h2>
            <div className="flex flex-col gap-3 rounded-lg border border-line bg-surface p-4">
                <div className="flex flex-col gap-1">
                    <p className="font-semibold break-words">
                        {t("settings.shortcut.label")}
                    </p>
                    <p id={hintId} className="text-sm break-words text-muted">
                        {t("settings.shortcut.hint")}
                    </p>
                </div>

                {capturing ? (
                    <div className="flex flex-wrap items-center gap-3">
                        <div
                            ref={captureRef}
                            role="textbox"
                            tabIndex={0}
                            aria-label={t("settings.shortcut.capturing")}
                            aria-describedby={captureHintId}
                            onKeyDown={onKeyDown}
                            onBlur={cancel}
                            style={MONO_FONT}
                            className="min-w-48 rounded-md border-2 border-dashed border-accent-edge bg-page px-3 py-1.5 focus-visible:outline-2 focus-visible:outline-offset-2 focus-visible:outline-focus"
                        >
                            {t("settings.shortcut.capturing")}
                        </div>
                        <button
                            type="button"
                            // Ends the pick before the field loses focus to this button.
                            onMouseDown={(event) => {
                                event.preventDefault();
                            }}
                            onClick={cancel}
                            className={BUTTON}
                        >
                            {t("settings.shortcut.cancel")}
                        </button>
                        <p
                            id={captureHintId}
                            className="w-full text-sm break-words text-muted"
                        >
                            {t("settings.shortcut.captureHint")}
                        </p>
                    </div>
                ) : (
                    <div className="flex flex-wrap items-center gap-3">
                        {shortcut.keys.length > 0 ? (
                            <Keys label={shortcutLabel(shortcut.keys)} />
                        ) : (
                            <span className="font-semibold">
                                {t("settings.shortcut.off")}
                            </span>
                        )}
                        <button
                            type="button"
                            disabled={busy}
                            onClick={begin}
                            aria-describedby={hintId}
                            className={BUTTON}
                        >
                            {shortcut.keys.length > 0
                                ? t("settings.shortcut.change")
                                : t("settings.shortcut.choose")}
                        </button>
                        {shortcut.keys.length > 0 && (
                            <button
                                type="button"
                                disabled={busy}
                                onClick={() => {
                                    void run("set_shortcut", { shortcut: null });
                                }}
                                className={BUTTON}
                            >
                                {t("settings.shortcut.turnOff")}
                            </button>
                        )}
                    </div>
                )}

                {shortcut.state === "unavailable" && (
                    <div className="flex flex-col gap-2">
                        {shortcut.error ? (
                            <ErrorBlock error={shortcut.error} />
                        ) : (
                            <p className="text-sm break-words">
                                {t("settings.shortcut.unavailable")}
                            </p>
                        )}
                        <div>
                            <button
                                type="button"
                                disabled={busy}
                                onClick={() => {
                                    void run("retry_shortcut");
                                }}
                                className={BUTTON}
                            >
                                {t("settings.shortcut.retry")}
                            </button>
                        </div>
                    </div>
                )}

                <p aria-live="polite" className="text-sm break-words text-muted">
                    {saved && !error && t("settings.shortcut.saved")}
                </p>
                {error && <ErrorBlock error={error} />}

                <fieldset className="flex flex-col gap-2">
                    <legend className="mb-1 font-semibold break-words">
                        {t("settings.shortcut.action.legend")}
                    </legend>
                    {(["keyboard", "selected"] as const).map((action) => (
                        <label
                            key={action}
                            className="flex items-start gap-2 break-words"
                        >
                            <input
                                type="radio"
                                name={actionName}
                                value={action}
                                checked={shortcut.action === action}
                                disabled={busy}
                                onChange={() => {
                                    setAction(action);
                                }}
                                className="mt-1 accent-accent"
                            />
                            <span className="flex flex-col">
                                <span>{t(`settings.shortcut.action.${action}`)}</span>
                                {action === "selected" && (
                                    <span className="text-sm text-muted">
                                        {t("settings.shortcut.action.selectedHint")}
                                    </span>
                                )}
                            </span>
                        </label>
                    ))}
                </fieldset>

                <div className="flex flex-col gap-1 border-t border-line pt-3">
                    <p className="flex flex-wrap items-center gap-x-2 break-words">
                        <span className="font-semibold">
                            {t("settings.emergency.label")}
                        </span>
                        <Keys
                            label={EMERGENCY_KEYS.map((key) => t(key)).join(
                                ` ${t("shortcut.key.plus")} `
                            )}
                        />
                    </p>
                    <p className="text-sm break-words text-muted">
                        {t("settings.emergency.hint")}
                    </p>
                </div>
            </div>
        </section>
    );
}
