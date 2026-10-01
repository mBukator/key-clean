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
    /** Length of the lock the Lock button requests, in seconds. */
    lockSeconds: number;
}

export interface Keyboard {
    id: string;
    name: string | null;
}

/** Name of the event the shell emits to the main window on every status change. */
export const STATUS_EVENT = "engine-status";
