import type { LockOptions } from "../../shared/types/engine";

/** The duration the user picked: a preset, or the Custom field's text (minutes and seconds). */
export type DurationChoice =
    | { kind: "preset"; seconds: number }
    | { kind: "custom"; minutes: string; seconds: string };

/** A choice that can be locked with, or why not. */
export type ParsedChoice =
    | { valid: true; seconds: number }
    | { valid: false; reason: "incomplete" | "outOfRange" };

const WHOLE = /^\d{1,3}$/;

/** The Custom field showing `total` seconds. */
export function customChoice(total: number): DurationChoice {
    return {
        kind: "custom",
        minutes: String(Math.floor(total / 60)),
        seconds: String(total % 60),
    };
}

/** The longest preset within the range (the 1 min preset always fits a max lock of 1 min). */
function longestFittingPreset(options: LockOptions): DurationChoice {
    const fitting = options.presetSeconds.filter((preset) => inRange(preset, options));
    const seconds = fitting.at(-1);
    return seconds === undefined
        ? customChoice(options.minSeconds)
        : { kind: "preset", seconds };
}

/**
 * The remembered choice: the last lock's duration, as a preset when it is one. Rust keeps it in
 * range; should it not be, the longest preset in range is used instead.
 */
export function initialChoice(options: LockOptions): DurationChoice {
    const seconds = options.lastSeconds;
    if (!inRange(seconds, options)) {
        return longestFittingPreset(options);
    }
    return options.presetSeconds.includes(seconds)
        ? { kind: "preset", seconds }
        : customChoice(seconds);
}

/** Whether `seconds` is within the range Rust accepts (10 s to the max-lock setting). */
export function inRange(seconds: number, options: LockOptions): boolean {
    return seconds >= options.minSeconds && seconds <= options.maxSeconds;
}

/**
 * The lock length of `choice`, checked against the same range Rust checks. Rust checks again;
 * this only drives the inline message and the Lock button.
 */
export function parseChoice(choice: DurationChoice, options: LockOptions): ParsedChoice {
    let seconds: number;
    if (choice.kind === "preset") {
        seconds = choice.seconds;
    } else {
        const minutesText = choice.minutes.trim() === "" ? "0" : choice.minutes.trim();
        const secondsText = choice.seconds.trim() === "" ? "0" : choice.seconds.trim();
        if (!WHOLE.test(minutesText) || !WHOLE.test(secondsText)) {
            return { valid: false, reason: "incomplete" };
        }
        const extra = Number(secondsText);
        if (extra > 59) {
            return { valid: false, reason: "incomplete" };
        }
        seconds = Number(minutesText) * 60 + extra;
    }
    return inRange(seconds, options)
        ? { valid: true, seconds }
        : { valid: false, reason: "outOfRange" };
}

/**
 * Keeps `choice` usable after the max-lock setting changed: a preset now out of range falls back
 * to the longest preset that still fits. Custom text is kept, so its inline message explains the
 * new limit.
 */
export function fitChoice(choice: DurationChoice, options: LockOptions): DurationChoice {
    if (choice.kind === "preset" && !inRange(choice.seconds, options)) {
        return longestFittingPreset(options);
    }
    return choice;
}
