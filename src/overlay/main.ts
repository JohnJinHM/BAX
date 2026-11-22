import "../shared/theme.css";
import "../shared/skins/index.css";
import "./overlay.css";

import { createApp } from "vue";

import { reportErrors } from "../shared/diagnostics";
import { restoreSkin } from "../shared/skin";
import OverlayApp from "./OverlayApp.vue";

restoreSkin();
const app = createApp(OverlayApp);
reportErrors(app);
app.mount("#app");
