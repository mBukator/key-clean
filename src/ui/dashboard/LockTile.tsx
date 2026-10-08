import { t, type StringKey } from "../../shared/localization/t";
import type { LockTargets, SessionState } from "../../shared/types/engine";
import { targetsKey } from "../../shared/ui/format";

const BUSY_KEYS: Record<Exclude<SessionState, "idle">, StringKey> = {
    starting: "lock.starting",
    locked: "panel.locked",
    unlocking: "status.unlocking",
};

const BIG =
    "flex min-h-28 w-full min-w-0 flex-col justify-between gap-4 rounded-xl border-2 border-b-[6px] px-5 py-4 text-left focus-visible:outline-2 focus-visible:outline-offset-2 focus-visible:outline-focus";

interface LockTileProps {
    state: SessionState;
    targets: LockTargets;
    canLock: boolean;
    pending: boolean;
    onLock: () => void;
    onUnlock: () => void;
}

/**
 * The primary action. At idle it starts a lock; during a lock it offers Unlock now, except when
 * the mouse is locked (it couldn't be clicked), where it points to the emergency shortcut.
 */
export function LockTile({
    state,
    targets,
    canLock,
    pending,
    onLock,
    onUnlock,
}: LockTileProps) {
    if (state === "locked" && !targets.mouse) {
        return (
            <button
                type="button"
                onClick={onUnlock}
                className={`${BIG} border-accent-edge bg-surface text-fg hover:bg-page`}
            >
                <span className="text-2xl font-semibold break-words">
                    {t("lock.unlockNow")}
                </span>
                <span className="text-sm break-words text-muted">
                    {t(targetsKey(targets))}
                </span>
            </button>
        );
    }
    if (state !== "idle") {
        // Starting, unlocking, or a mouse lock (no clickable unlock).
        return (
            <div
                className={`${BIG} border-surface-edge bg-surface text-fg`}
                aria-live="polite"
            >
                <span className="text-2xl font-semibold break-words">
                    {t(BUSY_KEYS[state])}
                </span>
                <span className="text-sm break-words text-muted">
                    {state === "locked" ? t("lock.lockedHint") : t(targetsKey(targets))}
                </span>
            </div>
        );
    }
    return (
        <button
            type="button"
            onClick={onLock}
            disabled={!canLock || pending}
            className={`${BIG} border-accent-edge bg-accent text-on-accent hover:brightness-105 disabled:cursor-not-allowed disabled:opacity-45 disabled:hover:brightness-100`}
        >
            <span className="text-3xl font-semibold break-words">
                {t(pending ? "lock.starting" : "lock.button")}
            </span>
            <span className="text-sm break-words">
                {targets.keyboard || targets.mouse
                    ? t(targetsKey(targets))
                    : t("error.no_targets")}
            </span>
        </button>
    );
}
