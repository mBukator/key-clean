import { t, type StringKey } from "../../shared/localization/t";
import type { Capability, Device, DeviceKind } from "../../shared/types/engine";
import { MONO_FONT } from "../../shared/ui/font";

type Group = "keyboard" | "mouse" | "touchpad" | "other";

const GROUPS: readonly { group: Group; titleKey: StringKey }[] = [
    { group: "keyboard", titleKey: "devices.group.keyboard" },
    { group: "mouse", titleKey: "devices.group.mouse" },
    { group: "touchpad", titleKey: "devices.group.touchpad" },
    { group: "other", titleKey: "devices.group.other" },
];

const CAPABILITY_KEYS: Record<Capability, StringKey> = {
    supported: "devices.capability.supported",
    limited: "devices.capability.limited",
    unsupported: "devices.capability.unsupported",
};

/** The badge always names the capability; the border style is a second cue, never the only one. */
const CAPABILITY_STYLES: Record<Capability, string> = {
    supported: "border-line bg-surface text-fg",
    limited: "border-fg/70 bg-transparent text-fg",
    unsupported: "border-dashed border-line bg-transparent text-muted",
};

/** Why a device isn't fully supported. Supported devices have no note. */
const NOTE_KEYS: Partial<Record<DeviceKind, StringKey>> = {
    touchpad: "devices.note.touchpad",
    touchscreen: "devices.note.touchscreen",
    pen: "devices.note.pen",
};

function groupOf(kind: DeviceKind): Group {
    return kind === "touchscreen" || kind === "pen" ? "other" : kind;
}

function DeviceRow({ device }: { device: Device }) {
    const noteKey =
        device.capability === "supported" ? undefined : NOTE_KEYS[device.kind];
    return (
        <li className="flex flex-col gap-1 border-b border-line py-2.5 last:border-b-0">
            <div className="flex flex-wrap items-center gap-x-3 gap-y-1">
                <span className="min-w-0 break-words">
                    {device.name ?? t("devices.unnamed")}
                </span>
                <span
                    style={MONO_FONT}
                    className={`rounded border px-1.5 py-px text-xs ${CAPABILITY_STYLES[device.capability]}`}
                >
                    {t(CAPABILITY_KEYS[device.capability])}
                </span>
            </div>
            {noteKey && <p className="text-sm break-words text-muted">{t(noteKey)}</p>}
        </li>
    );
}

/** The connected input devices, grouped by kind, with what KeyClean can do with each. */
export function DeviceList({ devices }: { devices: Device[] }) {
    if (devices.length === 0) {
        return <p className="text-sm text-muted">{t("devices.none")}</p>;
    }
    return (
        <div className="flex flex-col gap-5">
            {GROUPS.map(({ group, titleKey }) => {
                const members = devices.filter((d) => groupOf(d.kind) === group);
                if (members.length === 0) {
                    return null;
                }
                return (
                    <section key={group} className="flex flex-col gap-1">
                        <h3
                            style={MONO_FONT}
                            className="text-xs tracking-[0.12em] text-muted uppercase"
                        >
                            {t(titleKey)}
                        </h3>
                        <ul className="flex flex-col rounded-lg border border-line bg-surface px-3.5">
                            {members.map((d) => (
                                <DeviceRow key={d.id} device={d} />
                            ))}
                        </ul>
                    </section>
                );
            })}
        </div>
    );
}
