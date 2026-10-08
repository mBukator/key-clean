import { useId } from "react";

import { t } from "../../shared/localization/t";
import type { LockOptions } from "../../shared/types/engine";
import { MONO_FONT } from "../../shared/ui/font";
import { durationLabel, shortDurationLabel } from "../../shared/ui/format";
import { customChoice, inRange, parseChoice, type DurationChoice } from "./choice";

interface DurationPickerProps {
    options: LockOptions;
    choice: DurationChoice;
    onChange: (choice: DurationChoice) => void;
    disabled: boolean;
}

/** A keycap-like tile around a native radio button, so arrow keys move through the group. */
function RadioTile({
    label,
    accessibleLabel,
    checked,
    disabled,
    onSelect,
    name,
    describedBy,
}: {
    label: string;
    accessibleLabel: string;
    checked: boolean;
    disabled: boolean;
    onSelect: () => void;
    name: string;
    describedBy?: string;
}) {
    return (
        <label
            style={MONO_FONT}
            className={`flex min-h-12 min-w-0 cursor-pointer items-center justify-center rounded-lg border-2 border-b-4 px-3 py-2 text-center text-sm break-words has-focus-visible:outline-2 has-focus-visible:outline-offset-2 has-focus-visible:outline-focus has-disabled:cursor-not-allowed has-disabled:opacity-45 ${
                checked
                    ? "border-accent-edge bg-accent font-semibold text-on-accent"
                    : "border-surface-edge bg-surface text-fg hover:border-accent-edge"
            }`}
        >
            <input
                type="radio"
                name={name}
                checked={checked}
                disabled={disabled}
                onChange={onSelect}
                aria-label={accessibleLabel}
                aria-describedby={describedBy}
                className="sr-only"
            />
            <span aria-hidden="true">{label}</span>
        </label>
    );
}

/** Presets plus Custom (§11). Presets longer than the max-lock setting are turned off. */
export function DurationPicker({
    options,
    choice,
    onChange,
    disabled,
}: DurationPickerProps) {
    const name = useId();
    const hintId = useId();
    const messageId = useId();
    const overMaxId = useId();
    const custom = choice.kind === "custom";
    const parsed = parseChoice(choice, options);
    const anyOverMax = options.presetSeconds.some((preset) => !inRange(preset, options));
    const rangeMessage = t("duration.customRange", {
        min: durationLabel(options.minSeconds),
        max: durationLabel(options.maxSeconds),
    });

    return (
        <fieldset disabled={disabled} className="flex min-w-0 flex-col gap-2">
            <legend className="sr-only">{t("lock.durationLabel")}</legend>
            <div className="grid grid-cols-[repeat(auto-fit,minmax(5.25rem,1fr))] gap-2.5">
                {options.presetSeconds.map((preset) => (
                    <RadioTile
                        key={preset}
                        name={name}
                        label={shortDurationLabel(preset)}
                        accessibleLabel={durationLabel(preset)}
                        checked={choice.kind === "preset" && choice.seconds === preset}
                        disabled={!inRange(preset, options)}
                        describedBy={inRange(preset, options) ? undefined : overMaxId}
                        onSelect={() => {
                            onChange({ kind: "preset", seconds: preset });
                        }}
                    />
                ))}
                <RadioTile
                    name={name}
                    label={t("duration.custom")}
                    accessibleLabel={t("duration.customLegend")}
                    checked={custom}
                    disabled={false}
                    onSelect={() => {
                        onChange(
                            customChoice(
                                choice.kind === "preset"
                                    ? choice.seconds
                                    : options.lastSeconds
                            )
                        );
                    }}
                />
            </div>
            {custom && (
                <div className="flex flex-col gap-1.5 rounded-lg border border-line bg-surface p-3">
                    <p className="text-sm font-semibold">{t("duration.customLegend")}</p>
                    <div className="flex flex-wrap items-end gap-3">
                        {(["minutes", "seconds"] as const).map((field) => (
                            <label key={field} className="flex flex-col gap-1 text-sm">
                                <span className="text-muted">
                                    {t(
                                        field === "minutes"
                                            ? "duration.customMinutes"
                                            : "duration.customSeconds"
                                    )}
                                </span>
                                <input
                                    type="text"
                                    inputMode="numeric"
                                    autoComplete="off"
                                    maxLength={3}
                                    value={choice[field]}
                                    aria-invalid={!parsed.valid}
                                    aria-describedby={`${hintId} ${messageId}`}
                                    onChange={(event) => {
                                        onChange({
                                            ...choice,
                                            [field]: event.target.value,
                                        });
                                    }}
                                    style={MONO_FONT}
                                    className="w-20 rounded-md border-2 border-line bg-page px-2 py-1.5 text-base tabular-nums focus-visible:border-accent-edge focus-visible:outline-2 focus-visible:outline-offset-2 focus-visible:outline-focus aria-invalid:border-danger-line"
                                />
                            </label>
                        ))}
                    </div>
                    <p id={hintId} className="text-sm break-words text-muted">
                        {rangeMessage}
                    </p>
                    <p
                        id={messageId}
                        aria-live="polite"
                        className="text-sm font-medium break-words text-danger-fg empty:hidden"
                    >
                        {!parsed.valid && (
                            <>
                                <span aria-hidden="true" className="mr-1 font-bold">
                                    !
                                </span>
                                {parsed.reason === "incomplete"
                                    ? t("duration.customFormat")
                                    : t("duration.customOutOfRange")}
                            </>
                        )}
                    </p>
                </div>
            )}
            {anyOverMax && (
                <p id={overMaxId} className="text-sm break-words text-muted">
                    {t("duration.overMax", { max: durationLabel(options.maxSeconds) })}
                </p>
            )}
        </fieldset>
    );
}
