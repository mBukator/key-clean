import { useRef, type KeyboardEvent, type ReactNode } from "react";

export interface Tab<Id extends string> {
    id: Id;
    label: string;
}

interface TabsProps<Id extends string> {
    /** Accessible name of the tab list. */
    label: string;
    tabs: readonly Tab<Id>[];
    selected: Id;
    onSelect: (id: Id) => void;
    /** Shown at the end of the tab bar, outside the tab list. */
    trailing?: ReactNode;
    children: ReactNode;
}

const tabId = (id: string) => `tab-${id}`;
const panelId = (id: string) => `panel-${id}`;

/**
 * A WAI-ARIA tab list: one tab in the Tab order, arrow keys (and Home/End) move between tabs and
 * select them. Only the selected panel is rendered.
 */
export function Tabs<Id extends string>({
    label,
    tabs,
    selected,
    onSelect,
    trailing,
    children,
}: TabsProps<Id>) {
    const refs = useRef(new Map<Id, HTMLButtonElement>());

    const move = (event: KeyboardEvent<HTMLDivElement>) => {
        const index = tabs.findIndex((tab) => tab.id === selected);
        const last = tabs.length - 1;
        const next = {
            ArrowRight: index === last ? 0 : index + 1,
            ArrowLeft: index === 0 ? last : index - 1,
            Home: 0,
            End: last,
        }[event.key];
        const tab = next === undefined ? undefined : tabs[next];
        if (!tab) {
            return;
        }
        event.preventDefault();
        onSelect(tab.id);
        refs.current.get(tab.id)?.focus();
    };

    return (
        <div className="flex min-h-0 flex-1 flex-col">
            <div className="flex flex-wrap items-end justify-between gap-x-4 border-b border-line px-4 sm:px-8">
                <div
                    role="tablist"
                    aria-label={label}
                    onKeyDown={move}
                    className="-mb-px flex flex-wrap gap-x-6"
                >
                    {tabs.map((tab) => {
                        const active = tab.id === selected;
                        return (
                            <button
                                key={tab.id}
                                ref={(node) => {
                                    if (node) {
                                        refs.current.set(tab.id, node);
                                    } else {
                                        refs.current.delete(tab.id);
                                    }
                                }}
                                type="button"
                                role="tab"
                                id={tabId(tab.id)}
                                aria-selected={active}
                                aria-controls={active ? panelId(tab.id) : undefined}
                                tabIndex={active ? 0 : -1}
                                onClick={() => {
                                    onSelect(tab.id);
                                }}
                                className={`border-b-2 py-3 text-[0.95rem] break-words focus-visible:rounded-sm focus-visible:outline-2 focus-visible:outline-offset-2 focus-visible:outline-focus ${
                                    active
                                        ? "border-fg font-semibold text-fg"
                                        : "border-transparent text-muted hover:text-fg"
                                }`}
                            >
                                {tab.label}
                            </button>
                        );
                    })}
                </div>
                {trailing && <div className="py-2">{trailing}</div>}
            </div>
            <div
                role="tabpanel"
                id={panelId(selected)}
                aria-labelledby={tabId(selected)}
                className="flex min-h-0 flex-1 flex-col overflow-y-auto"
            >
                {children}
            </div>
        </div>
    );
}
