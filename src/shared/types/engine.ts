/** Mirrors `StatusDto` in src-tauri/src/bridge.rs. */
export type SessionState = "idle" | "starting" | "locked" | "unlocking";

export type EndReason =
    | "timeout"
    | "emergency"
    | "hardDeadline"
    | "suspend"
    | "endSession"
    | "sessionLock"
    | "sessionDisconnect"
    | "engineError"
    | "userRequest";

export interface ErrorInfo {
    /** Key into locales/en/strings.json. */
    messageKey: string;
    /** Technical details for "View technical details". */
    details: string;
}

export interface EngineStatus {
    state: SessionState;
    devCap: boolean;
    lastEndReason: EndReason | null;
    error: ErrorInfo | null;
    engineAvailable: boolean;
    /** Whole seconds left in the lock, rounded up, while starting or locked; otherwise null. */
    countdownSecs: number | null;
    /** Key into locales/en/strings.json for the latest notice, until the next lock starts. */
    notice: string | null;
}

/** Mirrors `LockOptionsDto` in src-tauri/src/bridge.rs. */
export interface LockOptions {
    /** The lock durations offered, in seconds, shortest first. */
    presetSeconds: number[];
    /** The preset selected by default, in seconds. */
    defaultSeconds: number;
}

export type DeviceKind = "keyboard" | "mouse" | "touchpad" | "touchscreen" | "pen";

/** How far KeyClean can control a device. */
export type Capability = "supported" | "limited" | "unsupported";

/** Mirrors `DeviceDto` in src-tauri/src/bridge.rs. */
export interface Device {
    id: string;
    name: string | null;
    kind: DeviceKind;
    capability: Capability;
}

/** Mirrors `DevicesDto` in src-tauri/src/bridge.rs. */
export interface DevicesState {
    devices: Device[];
    /** Set when listing failed (the old list is kept) or the list can't update by itself. */
    error: ErrorInfo | null;
}

/** Name of the event the shell emits to the main window on every status change. */
export const STATUS_EVENT = "engine-status";

/** Name of the event the shell emits to the main window when the device list changes. */
export const DEVICES_EVENT = "devices-changed";
