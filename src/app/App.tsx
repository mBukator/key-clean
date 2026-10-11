import { invoke } from "@tauri-apps/api/core";
import { useState, type ReactNode } from "react";

import { t } from "../shared/localization/t";
import type { ErrorInfo, LockOptions, LockTargets } from "../shared/types/engine";
import { ErrorBlock, toErrorInfo } from "../shared/ui/ErrorBlock";
import { MONO_FONT } from "../shared/ui/font";
import { Tabs, type Tab } from "../shared/ui/Tabs";
import { CleanScreen } from "../ui/dashboard/CleanScreen";
import { fitChoice, initialChoice, type DurationChoice } from "../ui/dashboard/choice";
import { DevicesScreen } from "../ui/devices/DevicesScreen";
import { SettingsScreen } from "../ui/settings/SettingsScreen";
import { useDevices, useEngineStatus, useLockOptions } from "./useEngine";

type TabId = "clean" | "devices" | "settings";

/**
 * The dashboard (§9). It only displays the engine's state and sends requests; the engine owns
 * every lock and its exits (invariant 3).
 */
export function App() {
    const [requestError, setRequestError] = useState<ErrorInfo | null>(null);
    const status = useEngineStatus(setRequestError);
    const devices = useDevices(setRequestError);
    const [options, setOptions] = useLockOptions(setRequestError);
    const [tab, setTab] = useState<TabId>("clean");
    // null until the user changes them: the remembered choices from the options apply.
    const [choice, setChoice] = useState<DurationChoice | null>(null);
    const [targets, setTargets] = useState<LockTargets | null>(null);
    const [pending, setPending] = useState(false);

    const tabs: readonly Tab<TabId>[] = [
        { id: "clean", label: t("tab.clean") },
        { id: "devices", label: t("tab.devices") },
        { id: "settings", label: t("tab.settings") },
    ];

    const lock = (seconds: number, lockTargets: LockTargets) => {
        setRequestError(null);
        // Waits for the overlay to confirm it is visible (up to about 2 s).
        setPending(true);
        invoke("lock_input", { seconds, ...lockTargets }).then(
            () => {
                setPending(false);
            },
            (err: unknown) => {
                setPending(false);
                setRequestError(toErrorInfo(err));
            }
        );
    };

    const unlock = () => {
        setRequestError(null);
        invoke("unlock_input").catch((err: unknown) => {
            setRequestError(toErrorInfo(err));
        });
    };

    const updateOptions = (updated: LockOptions) => {
        setOptions(updated);
        setChoice((current) => (current ? fitChoice(current, updated) : current));
    };

    const touchpad = devices?.devices.some((d) => d.kind === "touchpad") ?? false;
    const error = requestError ?? status?.error ?? null;
    const devCap = status?.devCap === true && (
        <span
            title={t("dev.capHint")}
            style={MONO_FONT}
            className="rounded border border-accent-edge px-2 py-0.5 text-xs font-bold text-fg"
        >
            {t("dev.cap")}
            <span className="sr-only">
                {t("dev.capSpoken", { hint: t("dev.capHint") })}
            </span>
        </span>
    );

    let panel: ReactNode;
    if (tab === "devices") {
        panel = <DevicesScreen devices={devices} />;
    } else if (!options) {
        panel = error && (
            <div className="px-4 py-5 sm:px-8">
                <ErrorBlock error={error} />
            </div>
        );
    } else if (tab === "settings") {
        panel = <SettingsScreen options={options} onOptions={updateOptions} />;
    } else {
        const currentTargets = targets ?? options.lastTargets;
        panel = (
            <CleanScreen
                status={status}
                options={options}
                choice={choice ?? initialChoice(options)}
                onChoice={setChoice}
                targets={currentTargets}
                onTargets={setTargets}
                touchpad={touchpad}
                pending={pending}
                error={error}
                onLock={(seconds) => {
                    lock(seconds, currentTargets);
                }}
                onUnlock={unlock}
            />
        );
    }

    return (
        <main className="flex h-dvh flex-col bg-page font-ui text-fg">
            <h1 className="sr-only">{t("app.title")}</h1>
            <Tabs
                label={t("tabs.label")}
                tabs={tabs}
                selected={tab}
                onSelect={setTab}
                trailing={devCap}
            >
                {panel}
            </Tabs>
        </main>
    );
}
