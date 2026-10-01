import { t } from "../shared/localization/t";

export function App() {
    return (
        <main className="flex min-h-screen flex-col items-center justify-center gap-2 p-6 text-center">
            <h1 className="text-3xl font-semibold">{t("app.title")}</h1>
            <p className="text-base text-neutral-600">{t("app.tagline")}</p>
        </main>
    );
}
