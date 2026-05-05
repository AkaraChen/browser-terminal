import Alpine from "https://cdn.jsdelivr.net/npm/alpinejs@3.15.12/dist/module.esm.js";

import { connectionStatus } from "./js/components/connection-status.js";
import { settingsDialog } from "./js/components/settings-dialog.js";
import { terminalPane } from "./js/components/terminal-pane.js";
import { terminalToolbar } from "./js/components/terminal-toolbar.js";
import { terminalStore } from "./js/terminal-store.js";

window.Alpine = Alpine;

Alpine.store("terminal", terminalStore());
Alpine.data("connectionStatus", connectionStatus);
Alpine.data("terminalToolbar", terminalToolbar);
Alpine.data("terminalPane", terminalPane);
Alpine.data("settingsDialog", settingsDialog);
Alpine.start();
