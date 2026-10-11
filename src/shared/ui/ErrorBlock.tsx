import { isStringKey, t } from "../localization/t";
import type { ErrorInfo } from "../types/engine";

/** An `ErrorInfo` from whatever a Tauri `invoke` rejected with. */
export function toErrorInfo(err: unknown): ErrorInfo {
    if (typeof err === "object" && err !== null && "messageKey" in err) {
        const { messageKey, details } = err as { messageKey: unknown; details?: unknown };
        if (typeof messageKey === "string") {
            return { messageKey, details: typeof details === "string" ? details : "" };
        }
    }
    return { messageKey: "error.ipc", details: String(err) };
}

/**
 * The shared error pattern (§46): a plain-language sentence, plus the raw details under "View
 * technical details". Never a bare error code.
 */
export function ErrorBlock({ error }: { error: ErrorInfo }) {
    const message = isStringKey(error.messageKey)
        ? t(error.messageKey)
        : t("error.unknown");
    return (
        <div
            role="alert"
            className="flex flex-col gap-2 rounded-lg border-2 border-danger-line bg-danger-bg p-3 text-danger-fg"
        >
            <p className="flex gap-2 break-words">
                <span aria-hidden="true" className="font-bold">
                    !
                </span>
                <span className="min-w-0">{message}</span>
            </p>
            {error.details && (
                <details className="text-sm">
                    <summary className="cursor-pointer rounded font-medium underline-offset-2 hover:underline focus-visible:outline-2 focus-visible:outline-offset-2 focus-visible:outline-focus">
                        {t("error.details")}
                    </summary>
                    <pre className="mt-2 font-mono text-xs break-words whitespace-pre-wrap">
                        {error.details}
                    </pre>
                </details>
            )}
        </div>
    );
}
