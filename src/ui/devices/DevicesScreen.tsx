import { t } from "../../shared/localization/t";
import type { DevicesState } from "../../shared/types/engine";
import { ErrorBlock } from "../../shared/ui/ErrorBlock";
import { DeviceList } from "./DeviceList";

/** The Devices tab: the live device list (§24), informational only (ADR 0004). */
export function DevicesScreen({ devices }: { devices: DevicesState | null }) {
    return (
        <div className="flex flex-col gap-4 px-4 py-5 sm:px-8 sm:py-6">
            <p className="text-sm break-words text-muted">{t("devices.intro")}</p>
            {devices?.error && <ErrorBlock error={devices.error} />}
            {devices && <DeviceList devices={devices.devices} />}
            <p className="text-sm break-words text-muted">{t("devices.lockScope")}</p>
        </div>
    );
}
