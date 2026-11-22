import "../shared/theme.css";
import "../shared/skins/index.css";
import "./main.css";

import { createApp } from "vue";

import { reportErrors } from "../shared/diagnostics";
import { restoreSkin } from "../shared/skin";
import MainApp from "./MainApp.vue";

restoreSkin();
const app = createApp(MainApp);
reportErrors(app);
app.mount("#app");
