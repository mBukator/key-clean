import { t } from "../../shared/localization/t";

const SEGMENTS = 50;

interface SegmentedProgressProps {
    remaining: number;
    total: number;
    accent: boolean;
}

/** A row of segments; the filled share is the time left of the whole lock. */
export function SegmentedProgress({ remaining, total, accent }: SegmentedProgressProps) {
    const share = total > 0 ? Math.min(Math.max(remaining / total, 0), 1) : 0;
    const filled = Math.ceil(share * SEGMENTS);
    const fill = accent ? "bg-[#f5a54a]" : "bg-[#a8a29b]";

    return (
        <div
            role="progressbar"
            aria-label={t("overlay.progressLabel")}
            aria-valuemin={0}
            aria-valuemax={total}
            aria-valuenow={Math.max(Math.min(remaining, total), 0)}
            className="flex w-full gap-1"
        >
            {Array.from({ length: SEGMENTS }, (_, index) => (
                <span
                    key={index}
                    className={`h-1 min-w-0 flex-1 rounded-sm ${index < filled ? fill : "bg-[#2a2723]"}`}
                />
            ))}
        </div>
    );
}
