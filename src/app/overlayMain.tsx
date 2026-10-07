import { StrictMode } from "react";
import { createRoot } from "react-dom/client";
import { Overlay } from "../ui/overlay/Overlay";
import "../index.css";

const root = document.getElementById("root");
if (root) {
    createRoot(root).render(
        <StrictMode>
            <Overlay />
        </StrictMode>
    );
}
