import { invoke } from "@tauri-apps/api/core";
import { useEffect, useId, useState } from "react";

import { t } from "../../shared/localization/t";
import type { ErrorInfo, GeneralSettings as General } from "../../shared/types/engine";
import { ErrorBlock, toErrorInfo } from "../../shared/ui/ErrorBlock";
import { MONO_FONT } from "../../shared/ui/font";

interface SwitchProps {
    label: string;
    hint: string;
    checked: boolean;
    disabled: boolean;
    onChange: (on: boolean) => void;
    /** Status lines, announced when they change; empty for none. */
    notes?: string[];
}

/**
 * An on/off switch. Only the label names it; the hint and the status lines describe it, and the
 * status region always exists, so a line added later is announced.
 */
function Switch({ label, hint, checked, disabled, onChange, notes = [] }: SwitchProps) {
    const inputId = useId();
    const hintId = useId();
    const notesId = useId();
    return (
        <div className="flex items-start gap-3 break-words">
            <input
                id={inputId}
                type="checkbox"
                role="switch"
                checked={checked}
                disabled={disabled}
                aria-describedby={`${hintId} ${notesId}`}
                onChange={(event) => {
                    onChange(event.target.checked);
                }}
                className="mt-1 size-4 shrink-0 accent-accent focus-visible:outline-2 focus-visible:outline-offset-2 focus-visible:outline-focus disabled:cursor-not-allowed"
            />
            <div className="flex min-w-0 flex-col gap-0.5">
                <label htmlFor={inputId} className="font-semibold">
                    {label}
                </label>
                <p id={hintId} className="text-sm text-muted">
                    {hint}
                </p>
                <div id={notesId} role="status" className="text-sm font-medium">
                    {notes.map((note) => (
                        <p key={note}>{note}</p>
                    ))}
                </div>
            </div>
        </div>
    );
}

/** The status lines under Start with Windows, for what the switch alone can't show. */
function startNotes(general: General): string[] {
    const notes: string[] = [];
    if (!general.startWithWindowsAvailable) {
        notes.push(t("settings.startWithWindows.devBuild"));
    }
    if (general.startWithWindows === "disabledInTaskManager") {
        notes.push(t("settings.startWithWindows.disabledInTaskManager"));
    } else if (
        general.startWithWindows === "otherCopy" &&
        general.startWithWindowsAvailable
    ) {
        // It asks the user to turn the switch on, which a dev build can't.
        notes.push(t("settings.startWithWindows.otherCopy"));
    }
    return notes;
}

/**
 * Settings → General (§29, §31, §32, ADR 0018). Starting with Windows is read from Windows each
 * time this page asks, so it always shows what Windows will do; it can only be changed in release
 * builds. Plain styling until Max's design arrives.
 */
export function GeneralSettings() {
    const [general, setGeneral] = useState<General | null>(null);
    const [error, setError] = useState<ErrorInfo | null>(null);
    const [busy, setBusy] = useState(false);

    useEffect(() => {
        invoke<General>("get_general_settings").then(setGeneral, (err: unknown) => {
            setError(toErrorInfo(err));
        });
    }, []);

    const run = (command: string, on: boolean) => {
        setBusy(true);
        setError(null);
        invoke<General>(command, { on }).then(
            (updated) => {
                setGeneral(updated);
                setBusy(false);
            },
            (err: unknown) => {
                setError(toErrorInfo(err));
                setBusy(false);
            }
        );
    };

    // KeyClean's entry exists. Turned off in Task Manager it still shows on, with a note, so the
    // switch can remove it; switching it on again wouldn't change what Task Manager decided.
    const startOn =
        general?.startWithWindows === "on" ||
        general?.startWithWindows === "disabledInTaskManager";
    const shownError = error ?? general?.error ?? null;

    return (
        <section className="flex flex-col gap-3">
            <h2
                style={MONO_FONT}
                className="text-xs tracking-[0.12em] text-muted uppercase"
            >
                {t("settings.general.title")}
            </h2>
            {general && (
                <div className="flex flex-col gap-4 rounded-lg border border-line bg-surface p-4">
                    <Switch
                        label={t("settings.startWithWindows.label")}
                        hint={t("settings.startWithWindows.hint")}
                        checked={startOn}
                        disabled={busy || !general.startWithWindowsAvailable}
                        onChange={(on) => {
                            run("set_start_with_windows", on);
                        }}
                        notes={startNotes(general)}
                    />
                    <Switch
                        label={t("settings.startMinimized.label")}
                        hint={t("settings.startMinimized.hint")}
                        checked={general.startMinimized}
                        disabled={busy}
                        onChange={(on) => {
                            run("set_start_minimized", on);
                        }}
                    />
                    <Switch
                        label={t("settings.notifications.label")}
                        hint={t("settings.notifications.hint")}
                        checked={general.notifications}
                        disabled={busy}
                        onChange={(on) => {
                            run("set_notifications", on);
                        }}
                    />
                </div>
            )}
            {shownError && <ErrorBlock error={shownError} />}
        </section>
    );
}
