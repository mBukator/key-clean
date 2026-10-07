import { Fragment } from "react";

import { t, type StringKey } from "../../shared/localization/t";
import { MONO_FONT } from "./font";

const KEYS: StringKey[] = ["overlay.key.ctrl", "overlay.key.alt", "overlay.key.k"];

/** "Emergency unlock:" with the Ctrl + Alt + K key caps. */
export function EmergencyShortcut() {
    return (
        <div className="flex max-w-full flex-col items-end gap-3 text-right">
            <p className="text-sm font-semibold break-words">
                {t("overlay.emergency.label")}
            </p>
            <p className="flex flex-wrap items-center justify-end gap-2">
                {KEYS.map((key, index) => (
                    <Fragment key={key}>
                        {index > 0 && (
                            <span aria-hidden="true" className="text-xs text-[#8a857e]">
                                {t("overlay.key.plus")}
                            </span>
                        )}
                        <kbd
                            style={MONO_FONT}
                            className="min-w-10 rounded-md border border-[#34302b] bg-[#1c1a17] px-3 py-1.5 text-center text-sm font-semibold shadow-[0_2px_0_#000]"
                        >
                            {t(key)}
                        </kbd>
                    </Fragment>
                ))}
            </p>
            <p className="text-xs break-words text-[#a19d96]">
                {t("overlay.emergency.hint")}
            </p>
        </div>
    );
}
