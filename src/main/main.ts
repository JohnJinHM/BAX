import "../shared/theme.css";
import "./main.css";

import { createApp } from "vue";

import { reportErrors } from "../shared/diagnostics";
import MainApp from "./MainApp.vue";

const app = createApp(MainApp);
reportErrors(app);
app.mount("#app");
