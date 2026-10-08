import { t, type StringKey } from "../../shared/localization/t";
import type { LockTargets } from "../../shared/types/engine";
import { MONO_FONT } from "../../shared/ui/font";

const TILE =
    "flex min-h-[4.75rem] min-w-0 flex-col justify-between gap-3 rounded-lg border-2 border-b-4 px-3.5 py-3 text-left";

/** The square light: filled when on. The tile also says On/Off in words (not colour alone). */
function Light({ on }: { on: boolean }) {
    return (
        <span
            aria-hidden="true"
            className={`size-2.5 rounded-xs ${on ? "bg-accent shadow-[0_0_6px_var(--kc-accent)]" : "bg-muted/50"}`}
        />
    );
}

function ToggleTile({
    labelKey,
    on,
    disabled,
    onToggle,
}: {
    labelKey: StringKey;
    on: boolean;
    disabled: boolean;
    onToggle: () => void;
}) {
    return (
        <button
            type="button"
            aria-pressed={on}
            disabled={disabled}
            onClick={onToggle}
            className={`${TILE} border-surface-edge bg-surface text-fg hover:border-accent-edge focus-visible:outline-2 focus-visible:outline-offset-2 focus-visible:outline-focus disabled:cursor-not-allowed disabled:opacity-45`}
        >
            <span className="flex w-full items-center justify-between gap-2">
                <Light on={on} />
                <span aria-hidden="true" style={MONO_FONT} className="text-xs text-muted">
                    {t(on ? "tile.on" : "tile.off")}
                </span>
            </span>
            <span className="font-semibold break-words">{t(labelKey)}</span>
        </button>
    );
}

interface TargetTilesProps {
    targets: LockTargets;
    onChange: (targets: LockTargets) => void;
    disabled: boolean;
    /** Whether a touchpad is connected; its tile is shown only then. */
    touchpad: boolean;
}

/**
 * Keyboard and Mouse toggles. The touchpad can't be chosen on its own (a hook can't tell it from a
 * mouse, ADR 0004/0013), so its tile only mirrors the mouse and is marked Limited.
 */
export function TargetTiles({ targets, onChange, disabled, touchpad }: TargetTilesProps) {
    return (
        <div
            role="group"
            aria-label={t("tile.targetsLegend")}
            className="grid grid-cols-[repeat(auto-fit,minmax(8.5rem,1fr))] gap-2.5"
        >
            <ToggleTile
                labelKey="tile.keyboard"
                on={targets.keyboard}
                disabled={disabled}
                onToggle={() => {
                    onChange({ ...targets, keyboard: !targets.keyboard });
                }}
            />
            <ToggleTile
                labelKey="tile.mouse"
                on={targets.mouse}
                disabled={disabled}
                onToggle={() => {
                    onChange({ ...targets, mouse: !targets.mouse });
                }}
            />
            {touchpad && (
                <div
                    className={`${TILE} border-dashed border-line bg-transparent text-fg`}
                >
                    <span className="flex w-full flex-wrap items-center justify-between gap-2">
                        <Light on={targets.mouse} />
                        <span
                            style={MONO_FONT}
                            className="rounded border border-fg/70 px-1.5 py-px text-xs"
                        >
                            {t("tile.limited")}
                        </span>
                    </span>
                    <span className="flex flex-col">
                        <span className="font-semibold break-words">
                            {t("tile.touchpad")}
                        </span>
                        <span className="text-xs break-words text-muted">
                            {t("tile.followsMouse")}
                            {t("separator.dot")}
                            {t(targets.mouse ? "tile.on" : "tile.off")}
                        </span>
                    </span>
                </div>
            )}
        </div>
    );
}
