import { invoke } from "@tauri-apps/api/core";
import { getCurrentWebviewWindow } from "@tauri-apps/api/webviewWindow";
import { useEffect, useState } from "react";

import {
    DEVICES_EVENT,
    STATUS_EVENT,
    type DevicesState,
    type EngineStatus,
    type ErrorInfo,
    type LockOptions,
} from "../shared/types/engine";
import { toErrorInfo } from "../shared/ui/ErrorBlock";

/**
 * Subscribes to `event`, then asks `command` for the current value, so nothing in between is
 * missed. An event that arrives before the answer wins, since it is newer. Nothing polls.
 */
// T names the payload type for the callers; it is only used in the return type by design.
// eslint-disable-next-line @typescript-eslint/no-unnecessary-type-parameters
function useSubscribed<T>(
    event: string,
    command: string,
    onError: (error: ErrorInfo) => void
): T | null {
    const [value, setValue] = useState<T | null>(null);

    useEffect(() => {
        let unlisten: (() => void) | undefined;
        let disposed = false;

        getCurrentWebviewWindow()
            .listen<T>(event, (e) => {
                setValue(e.payload);
            })
            .then((fn) => {
                if (disposed) {
                    fn();
                    return undefined;
                }
                unlisten = fn;
                return invoke<T>(command).then((current) => {
                    setValue((latest) => latest ?? current);
                });
            })
            .catch((err: unknown) => {
                onError(toErrorInfo(err));
            });

        return () => {
            disposed = true;
            unlisten?.();
        };
    }, [event, command, onError]);

    return value;
}

/** The engine status, kept current by `engine-status` events. */
export function useEngineStatus(
    onError: (error: ErrorInfo) => void
): EngineStatus | null {
    return useSubscribed<EngineStatus>(STATUS_EVENT, "get_status", onError);
}

/** The device list, kept current by `devices-changed` events. */
export function useDevices(onError: (error: ErrorInfo) => void): DevicesState | null {
    return useSubscribed<DevicesState>(DEVICES_EVENT, "list_devices", onError);
}

/** The lock options (presets, range, remembered choices), read once. */
export function useLockOptions(
    onError: (error: ErrorInfo) => void
): [LockOptions | null, (options: LockOptions) => void] {
    const [options, setOptions] = useState<LockOptions | null>(null);

    useEffect(() => {
        let disposed = false;
        invoke<LockOptions>("get_lock_options")
            .then((current) => {
                if (!disposed) {
                    setOptions(current);
                }
            })
            .catch((err: unknown) => {
                onError(toErrorInfo(err));
            });
        return () => {
            disposed = true;
        };
    }, [onError]);

    return [options, setOptions];
}
