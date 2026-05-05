import { ensureChannelId, newChannelUrl } from "./channel.js";
import {
    ICON_BASE,
    SETTINGS_KEY,
    SYSTEM_FONT,
    TERMINAL_THEME,
    TITLE_ICONS,
} from "./config.js";
import {
    loadTerminalFont,
    persistSettings,
    readSettings,
} from "./settings.js";
import { createTerminalSocket, sendSocketData } from "./socket.js";

export function terminalStore() {
    return {
        channelId: "",
        encoder: new TextEncoder(),
        fitAddon: null,
        fontSizeInput: "",
        initialized: false,
        resizeObserver: null,
        settings: readSettings(),
        socket: null,
        status: "connecting",
        term: null,
        webFontsAddon: null,

        get statusClass() {
            return {
                connected: "bg-emerald-400",
                disconnected: "bg-red-400",
                connecting: "bg-zinc-500",
            }[this.status];
        },

        get statusLabel() {
            return {
                connected: "connected",
                disconnected: "disconnected",
                connecting: "connecting",
            }[this.status];
        },

        init() {
            if (this.initialized) return;

            this.initialized = true;
            this.channelId = ensureChannelId();
            this.syncSettingsForm();
            window.lucide?.createIcons();

            this.term = new window.Terminal({
                cursorBlink: true,
                convertEol: true,
                fontFamily: this.settings.fontFamily,
                fontSize: this.settings.fontSize,
                lineHeight: 1.1,
                scrollback: 5000,
                theme: TERMINAL_THEME,
            });
            this.fitAddon = new window.FitAddon.FitAddon();
            this.webFontsAddon = new window.WebFontsAddon.WebFontsAddon(false);
            this.term.loadAddon(this.fitAddon);
            this.term.loadAddon(this.webFontsAddon);

            this.term.onData((data) =>
                sendSocketData(this.socket, this.encoder, data),
            );
            this.term.onResize(() => this.sendResize());
            this.term.onTitleChange((title) => this.syncDocumentTitle(title));

            window.addEventListener("storage", (event) => {
                if (event.key !== SETTINGS_KEY) return;

                this.settings = readSettings();
                this.applySettings(this.settings);
                this.syncSettingsForm();
            });
        },

        async attachTerminal(element) {
            this.init();
            await loadTerminalFont(this.settings.fontFamily, this.webFontsAddon);
            this.term.open(element);
            this.term.element?.classList.add("h-full");
            this.fitAddon.fit();
            this.term.focus();
            this.connectSocket();

            this.resizeObserver = new ResizeObserver(() => {
                this.fitAddon.fit();
                this.sendResize();
            });
            this.resizeObserver.observe(element);
        },

        connectSocket() {
            this.socket = createTerminalSocket({
                channelId: this.channelId,
                term: this.term,
                setStatus: (status) => {
                    this.status = status;
                },
                sendResize: () => this.sendResize(),
            });
        },

        sendResize() {
            if (this.socket?.readyState !== WebSocket.OPEN) return;
            this.socket.send(
                JSON.stringify({
                    type: "resize",
                    cols: this.term.cols,
                    rows: this.term.rows,
                }),
            );
        },

        async applySettings(nextSettings) {
            const loaded = await loadTerminalFont(
                nextSettings.fontFamily,
                this.webFontsAddon,
            );
            this.term.options.fontFamily = loaded
                ? nextSettings.fontFamily
                : SYSTEM_FONT;
            this.term.options.fontSize = nextSettings.fontSize;
            requestAnimationFrame(() => {
                this.fitAddon.fit();
                this.term.refresh(0, this.term.rows - 1);
                this.sendResize();
            });
        },

        saveSettings(nextSettings, options = {}) {
            this.settings = persistSettings(nextSettings);
            this.applySettings(this.settings);
            if (options.syncForm !== false) {
                this.syncSettingsForm();
            }
        },

        syncSettingsForm() {
            this.fontSizeInput = String(this.settings.fontSize);
        },

        updateFontFamily(fontFamily) {
            this.saveSettings({ ...this.settings, fontFamily });
        },

        updateFontSize(fontSize) {
            this.saveSettings({ ...this.settings, fontSize });
        },

        updateFontSizeInput(fontSize) {
            this.fontSizeInput = fontSize;
            if (!fontSize) return;

            this.saveSettings(
                { ...this.settings, fontSize },
                { syncForm: false },
            );
        },

        async openNewTab() {
            try {
                const response = await fetch("/api/windows", {
                    method: "POST",
                    headers: {
                        "content-type": "application/json",
                    },
                    body: JSON.stringify({ channel: this.channelId }),
                });

                if (!response.ok) {
                    throw new Error(await response.text());
                }
            } catch (error) {
                console.error("failed to open terminal window", error);
                window.open(newChannelUrl().toString(), "_blank", "noopener");
            }
        },

        syncDocumentTitle(title) {
            document.title = title || "Browser Terminal";
            const match = TITLE_ICONS.find(([name]) =>
                document.title.toLowerCase().includes(name),
            );
            document.getElementById("app-icon").href = match
                ? `${ICON_BASE}/${match[1]}`
                : "data:,";
        },
    };
}
