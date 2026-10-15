import { invoke } from "@tauri-apps/api/core";
import { useId, useState, type SyntheticEvent } from "react";

import { t } from "../../shared/localization/t";
import type { ErrorInfo, LockOptions, ShortcutInfo } from "../../shared/types/engine";
import { ErrorBlock, toErrorInfo } from "../../shared/ui/ErrorBlock";
import { MONO_FONT } from "../../shared/ui/font";
import { ShortcutSettings } from "./ShortcutSettings";

interface SettingsScreenProps {
    options: LockOptions;
    onOptions: (options: LockOptions) => void;
    shortcut: ShortcutInfo | null;
}

/** Settings (§32): Shortcuts (M9) and Safety → Maximum lock duration (M7). */
export function SettingsScreen({ options, onOptions, shortcut }: SettingsScreenProps) {
    const inputId = useId();
    const hintId = useId();
    const messageId = useId();
    const [text, setText] = useState(String(options.maxLockMinutes));
    const [saved, setSaved] = useState(false);
    const [error, setError] = useState<ErrorInfo | null>(null);

    const min = options.minMaxLockMinutes;
    const max = options.maxMaxLockMinutes;
    const minutes = /^\d{1,2}$/.test(text.trim()) ? Number(text.trim()) : null;
    const valid = minutes !== null && minutes >= min && minutes <= max;
    const changed = valid && minutes !== options.maxLockMinutes;

    const save = (event: SyntheticEvent) => {
        event.preventDefault();
        if (!valid) {
            return;
        }
        setError(null);
        invoke<LockOptions>("set_max_lock", { minutes }).then(
            (updated) => {
                onOptions(updated);
                setText(String(updated.maxLockMinutes));
                setSaved(true);
            },
            (err: unknown) => {
                setError(toErrorInfo(err));
            }
        );
    };

    return (
        <div className="flex flex-col gap-6 px-4 py-5 sm:px-8 sm:py-6">
            {shortcut && <ShortcutSettings shortcut={shortcut} />}
            <section className="flex flex-col gap-3">
                <h2
                    style={MONO_FONT}
                    className="text-xs tracking-[0.12em] text-muted uppercase"
                >
                    {t("settings.safety.title")}
                </h2>
                <form
                    onSubmit={save}
                    className="flex flex-col gap-2 rounded-lg border border-line bg-surface p-4"
                >
                    <label htmlFor={inputId} className="font-semibold break-words">
                        {t("settings.maxLock.label")}
                    </label>
                    <p id={hintId} className="text-sm break-words text-muted">
                        {t("settings.maxLock.hint", { min, max })}
                    </p>
                    <div className="flex flex-wrap items-center gap-3">
                        <span className="flex items-center gap-2">
                            <input
                                id={inputId}
                                type="text"
                                inputMode="numeric"
                                autoComplete="off"
                                maxLength={2}
                                value={text}
                                aria-invalid={!valid}
                                aria-describedby={`${hintId} ${messageId}`}
                                onChange={(event) => {
                                    setText(event.target.value);
                                    setSaved(false);
                                }}
                                style={MONO_FONT}
                                className="w-16 rounded-md border-2 border-line bg-page px-2 py-1.5 text-base tabular-nums focus-visible:border-accent-edge focus-visible:outline-2 focus-visible:outline-offset-2 focus-visible:outline-focus aria-invalid:border-danger-line"
                            />
                            <span className="text-sm text-muted">
                                {t("settings.maxLock.unit")}
                            </span>
                        </span>
                        <button
                            type="submit"
                            disabled={!changed}
                            className="rounded-md border-2 border-b-4 border-accent-edge bg-accent px-4 py-1.5 font-semibold text-on-accent focus-visible:outline-2 focus-visible:outline-offset-2 focus-visible:outline-focus disabled:cursor-not-allowed disabled:opacity-45"
                        >
                            {t("settings.maxLock.save")}
                        </button>
                    </div>
                    <p id={messageId} aria-live="polite" className="text-sm break-words">
                        {!valid && (
                            <span className="font-medium text-danger-fg">
                                <span aria-hidden="true" className="mr-1 font-bold">
                                    !
                                </span>
                                {t("settings.maxLock.invalid", { min, max })}
                            </span>
                        )}
                        {valid && saved && (
                            <span className="text-muted">
                                {t("settings.maxLock.saved")}
                            </span>
                        )}
                    </p>
                </form>
                {error && <ErrorBlock error={error} />}
            </section>
        </div>
    );
}
