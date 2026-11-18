import "../shared/theme.css";
import "./overlay.css";

import { createApp } from "vue";

import { reportErrors } from "../shared/diagnostics";
import OverlayApp from "./OverlayApp.vue";

const app = createApp(OverlayApp);
reportErrors(app);
app.mount("#app");
