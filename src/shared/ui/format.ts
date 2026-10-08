import { t, type StringKey } from "../localization/t";
import type { LockTargets } from "../types/engine";

const plurals = new Intl.PluralRules("en");

function form(count: number): "one" | "other" {
    return plurals.select(count) === "one" ? "one" : "other";
}

const DURATION_KEYS = {
    seconds: { one: "duration.seconds.one", other: "duration.seconds.other" },
    minutes: { one: "duration.minutes.one", other: "duration.minutes.other" },
} satisfies Record<string, Record<"one" | "other", StringKey>>;

/** "30 seconds", "1 minute", "5 minutes", "90 seconds". */
export function durationLabel(seconds: number): string {
    const unit = seconds % 60 === 0 ? "minutes" : "seconds";
    const count = unit === "minutes" ? seconds / 60 : seconds;
    return t(DURATION_KEYS[unit][form(count)], { count });
}

/** "30 s", "1 min", "5 min": the preset buttons. */
export function shortDurationLabel(seconds: number): string {
    return seconds % 60 === 0
        ? t("duration.short.minutes", { count: seconds / 60 })
        : t("duration.short.seconds", { count: seconds });
}

/** "02:00", "29:59" (§15). */
export function formatCountdown(seconds: number): string {
    const minutes = Math.floor(seconds / 60);
    const rest = seconds % 60;
    return `${String(minutes).padStart(2, "0")}:${String(rest).padStart(2, "0")}`;
}

/** "Keyboard", "Mouse and touchpad", "Keyboard, mouse and touchpad". */
export function targetsKey(targets: LockTargets): StringKey {
    if (targets.mouse) {
        return targets.keyboard ? "targets.all" : "targets.mouse";
    }
    return "targets.keyboard";
}
