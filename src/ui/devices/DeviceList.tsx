import { t, type StringKey } from "../../shared/localization/t";
import type { Capability, Device, DeviceKind } from "../../shared/types/engine";

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

const CAPABILITY_STYLES: Record<Capability, string> = {
    supported: "bg-green-100 text-green-900",
    limited: "bg-amber-100 text-amber-900",
    unsupported: "bg-neutral-200 text-neutral-800",
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
        <li className="flex flex-col gap-1">
            <div className="flex flex-wrap items-center gap-x-3 gap-y-1">
                <span className="break-words">{device.name ?? t("devices.unnamed")}</span>
                <span
                    className={`rounded px-2 py-0.5 text-xs font-medium ${CAPABILITY_STYLES[device.capability]}`}
                >
                    {t(CAPABILITY_KEYS[device.capability])}
                </span>
            </div>
            {noteKey && (
                <p className="text-xs break-words text-neutral-600">{t(noteKey)}</p>
            )}
        </li>
    );
}

/** The connected input devices, grouped by kind, with what KeyClean can do with each. */
export function DeviceList({ devices }: { devices: Device[] }) {
    if (devices.length === 0) {
        return <p className="text-sm text-neutral-600">{t("devices.none")}</p>;
    }
    return (
        <div className="flex flex-col gap-4">
            {GROUPS.map(({ group, titleKey }) => {
                const members = devices.filter((d) => groupOf(d.kind) === group);
                if (members.length === 0) {
                    return null;
                }
                return (
                    <section key={group} className="flex flex-col gap-1">
                        <h3 className="text-sm font-semibold text-neutral-700">
                            {t(titleKey)}
                        </h3>
                        <ul className="flex flex-col gap-2">
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
