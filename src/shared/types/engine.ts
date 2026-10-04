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
    /** Counts keyboard connects and disconnects; a change means the keyboard list is stale. */
    deviceChanges: number;
}

/** Mirrors `LockOptionsDto` in src-tauri/src/bridge.rs. */
export interface LockOptions {
    /** The lock durations offered, in seconds, shortest first. */
    presetSeconds: number[];
    /** The preset selected by default, in seconds. */
    defaultSeconds: number;
}

export interface Keyboard {
    id: string;
    name: string | null;
}

/** Name of the event the shell emits to the main window on every status change. */
export const STATUS_EVENT = "engine-status";
