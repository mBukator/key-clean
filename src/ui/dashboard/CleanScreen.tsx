import { isStringKey, t, type StringKey } from "../../shared/localization/t";
import type {
    EndReason,
    EngineStatus,
    ErrorInfo,
    LockOptions,
    LockTargets,
    ShortcutInfo,
} from "../../shared/types/engine";
import { ErrorBlock } from "../../shared/ui/ErrorBlock";
import { parseChoice, type DurationChoice } from "./choice";
import { DurationPicker } from "./DurationPicker";
import { LockTile } from "./LockTile";
import { StatusPanel } from "./StatusPanel";
import { TargetTiles } from "./TargetTiles";

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

interface CleanScreenProps {
    status: EngineStatus | null;
    options: LockOptions;
    choice: DurationChoice;
    onChoice: (choice: DurationChoice) => void;
    targets: LockTargets;
    onTargets: (targets: LockTargets) => void;
    /** Whether a touchpad is connected. */
    touchpad: boolean;
    shortcut: ShortcutInfo | null;
    pending: boolean;
    error: ErrorInfo | null;
    onLock: (seconds: number) => void;
    onUnlock: () => void;
}

/** The main screen (§10): pick a duration and what to lock, then Lock. */
export function CleanScreen({
    status,
    options,
    choice,
    onChoice,
    targets,
    onTargets,
    touchpad,
    shortcut,
    pending,
    error,
    onLock,
    onUnlock,
}: CleanScreenProps) {
    const state = status?.state ?? "idle";
    const idle = state === "idle";
    const parsed = parseChoice(choice, options);
    const chosenSeconds = parsed.valid ? parsed.seconds : null;
    const canLock =
        status?.engineAvailable === true &&
        idle &&
        chosenSeconds !== null &&
        (targets.keyboard || targets.mouse);
    // During a lock, show what the engine says; at idle, what the user picked.
    const shownTargets = !idle && status?.targets ? status.targets : targets;
    const shownSeconds = idle ? chosenSeconds : (status?.countdownSecs ?? null);
    const notice = status?.notice && isStringKey(status.notice) ? t(status.notice) : null;
    const lastEnd = idle ? (status?.lastEndReason ?? null) : null;

    return (
        <div className="flex flex-col gap-4 px-4 py-5 sm:px-8 sm:py-6">
            <StatusPanel
                state={state}
                seconds={shownSeconds}
                targets={shownTargets}
                touchpad={touchpad}
                shortcut={shortcut}
            />

            <div className="grid gap-3 sm:grid-cols-[minmax(0,1fr)_minmax(9.5rem,11rem)]">
                <div className="flex min-w-0 flex-col gap-3">
                    <DurationPicker
                        options={options}
                        choice={choice}
                        onChange={onChoice}
                        disabled={!idle || pending}
                    />
                    <TargetTiles
                        targets={targets}
                        onChange={onTargets}
                        disabled={!idle || pending}
                        touchpad={touchpad}
                    />
                </div>
                <div className="flex">
                    <LockTile
                        state={state}
                        targets={shownTargets}
                        canLock={canLock}
                        pending={pending}
                        onLock={() => {
                            if (chosenSeconds !== null) {
                                onLock(chosenSeconds);
                            }
                        }}
                        onUnlock={onUnlock}
                    />
                </div>
            </div>

            {lastEnd && (
                <p
                    aria-live="polite"
                    className="rounded-lg border border-line bg-surface p-3 break-words"
                >
                    {lastEnd === "timeout"
                        ? t("status.complete")
                        : t("status.lastEnd", { reason: t(END_REASON_KEYS[lastEnd]) })}
                </p>
            )}
            {notice && (
                <p
                    aria-live="polite"
                    className="rounded-lg border border-notice-line bg-notice-bg p-3 break-words text-notice-fg"
                >
                    {notice}
                </p>
            )}
            {error && <ErrorBlock error={error} />}

            <p className="text-sm break-words text-muted">
                {t("clean.note.keyboards")}
                {touchpad && ` ${t("clean.note.touchpad")}`}
            </p>
            {targets.mouse && idle && (
                <p className="text-sm break-words text-muted">
                    {t("lock.target.mouseHint")}
                </p>
            )}
        </div>
    );
}
