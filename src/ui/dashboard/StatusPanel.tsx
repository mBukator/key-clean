import { Fragment } from "react";

import { t, type StringKey } from "../../shared/localization/t";
import type { LockTargets, SessionState, ShortcutInfo } from "../../shared/types/engine";
import { MONO_FONT } from "../../shared/ui/font";
import { shortcutLabel } from "../../shared/ui/shortcutKeys";
import { formatCountdown } from "../../shared/ui/format";

const LABEL_KEYS: Record<SessionState, StringKey[]> = {
    idle: ["panel.ready", "panel.lockFor"],
    starting: ["panel.starting"],
    locked: ["panel.locked", "panel.timeLeft"],
    unlocking: ["panel.unlocking"],
};

const KEYS: StringKey[] = ["overlay.key.ctrl", "overlay.key.alt", "overlay.key.k"];

interface StatusPanelProps {
    state: SessionState;
    /** Seconds to show: the chosen length at idle, the countdown during a lock. */
    seconds: number | null;
    targets: LockTargets;
    /** Whether a touchpad is connected; its row is shown only then. */
    touchpad: boolean;
    /** The start shortcut, shown under the emergency shortcut. */
    shortcut: ShortcutInfo | null;
}

/** The start shortcut line: its keys, off, or not working. Hidden while picking a new one. */
function ShortcutLine({ shortcut }: { shortcut: ShortcutInfo }) {
    if (shortcut.state === "capturing") {
        return null;
    }
    if (shortcut.state === "off") {
        return <span className="text-panel-label">{t("clean.shortcutOff")}</span>;
    }
    if (shortcut.state === "unavailable") {
        return (
            <span className="text-panel-label normal-case">
                {t("clean.shortcutUnavailable")}
            </span>
        );
    }
    return (
        <>
            <span className="text-panel-label">{t("clean.shortcut")}</span>
            <span className="font-semibold">{shortcutLabel(shortcut.keys)}</span>
        </>
    );
}

/** The dark display: state, time, what gets locked, and the emergency shortcut (§18). */
export function StatusPanel({
    state,
    seconds,
    targets,
    touchpad,
    shortcut,
}: StatusPanelProps) {
    const rows: { label: StringKey; value: StringKey }[] = [
        { label: "tile.keyboard", value: targets.keyboard ? "panel.on" : "panel.off" },
        { label: "tile.mouse", value: targets.mouse ? "panel.on" : "panel.off" },
    ];
    if (touchpad) {
        rows.push({
            label: "tile.touchpad",
            value: targets.mouse ? "panel.limited" : "panel.off",
        });
    }
    const locked = state === "locked";

    return (
        <section
            aria-label={t("app.title")}
            style={MONO_FONT}
            className="flex flex-col gap-4 rounded-xl border border-panel-line bg-panel px-5 pt-5 pb-4 text-panel-value sm:px-6"
        >
            <div className="flex flex-wrap items-end justify-between gap-x-8 gap-y-4">
                <div className="flex min-w-0 flex-col gap-2">
                    <p
                        aria-live="polite"
                        className="text-xs tracking-[0.2em] break-words text-panel-label uppercase"
                    >
                        {LABEL_KEYS[state].map((key) => t(key)).join(t("separator.dot"))}
                    </p>
                    <p
                        role={locked ? "timer" : undefined}
                        className="text-[clamp(3.25rem,11vw,5.5rem)] leading-none font-light tabular-nums"
                    >
                        {seconds === null
                            ? t("panel.noDuration")
                            : formatCountdown(seconds)}
                    </p>
                </div>
                <dl
                    aria-label={t("panel.targets")}
                    className="grid grid-cols-[auto_auto] gap-x-5 gap-y-1.5 text-xs tracking-[0.12em] uppercase"
                >
                    {rows.map((row) => (
                        <Fragment key={row.label}>
                            <dt className="text-panel-label">{t(row.label)}</dt>
                            <dd className="font-semibold">{t(row.value)}</dd>
                        </Fragment>
                    ))}
                </dl>
            </div>
            <p className="flex flex-wrap items-center gap-x-2 gap-y-1 border-t border-panel-line pt-3 text-xs tracking-[0.12em] uppercase">
                <span className="text-panel-label">{t("panel.emergency")}</span>
                <span className="font-semibold">
                    {KEYS.map((key) => t(key)).join(` ${t("overlay.key.plus")} `)}
                </span>
            </p>
            {shortcut && (
                <p className="-mt-2 flex flex-wrap items-center gap-x-2 gap-y-1 text-xs tracking-[0.12em] uppercase">
                    <ShortcutLine shortcut={shortcut} />
                </p>
            )}
        </section>
    );
}
