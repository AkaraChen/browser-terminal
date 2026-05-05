export const SETTINGS_KEY = "browser-terminal:settings";

export const ICON_BASE =
    "https://cdn.jsdelivr.net/npm/@lobehub/icons-static-svg@1/icons";

export const TITLE_ICONS = [
    ["claude", "claude-color.svg"],
    ["codex", "codex-color.svg"],
    ["opencode", "opencode.svg"],
    ["qoder", "qoder-color.svg"],
    ["amp", "amp-color.svg"],
    ["cline", "cline.svg"],
    ["copilot", "copilot-color.svg"],
    ["cursor", "cursor.svg"],
    ["kilo", "kilocode.svg"],
    ["kimi", "kimi-color.svg"],
];

export const NERD_FONTS_BASE =
    "https://cdn.jsdelivr.net/gh/ryanoasis/nerd-fonts@v3.3.0/patched-fonts";

export const SYSTEM_FONT =
    'ui-monospace, SFMono-Regular, Menlo, Monaco, Consolas, "Liberation Mono", monospace';

export const TERMINAL_FONTS = [
    {
        label: "JetBrainsMono Nerd Font",
        family: "JetBrainsMono Nerd Font",
        face: new FontFace(
            "JetBrainsMono Nerd Font",
            `url(${NERD_FONTS_BASE}/JetBrainsMono/Ligatures/Regular/JetBrainsMonoNerdFont-Regular.ttf)`,
        ),
    },
    {
        label: "MesloLGM Nerd Font",
        family: "MesloLGM Nerd Font",
        face: new FontFace(
            "MesloLGM Nerd Font",
            `url(${NERD_FONTS_BASE}/Meslo/M/Regular/MesloLGMNerdFont-Regular.ttf)`,
        ),
    },
    {
        label: "系统等宽",
        family: SYSTEM_FONT,
    },
];

export const DEFAULT_SETTINGS = {
    fontFamily: TERMINAL_FONTS[0].family,
    fontSize: 14,
};

export const TERMINAL_THEME = {
    background: "#111318",
    foreground: "#e9edf3",
    cursor: "#e9edf3",
    selectionBackground: "#3f5268",
    black: "#111318",
    red: "#e06c75",
    green: "#98c379",
    yellow: "#e5c07b",
    blue: "#61afef",
    magenta: "#c678dd",
    cyan: "#56b6c2",
    white: "#d7dae0",
    brightBlack: "#5c6370",
    brightRed: "#e06c75",
    brightGreen: "#98c379",
    brightYellow: "#e5c07b",
    brightBlue: "#61afef",
    brightMagenta: "#c678dd",
    brightCyan: "#56b6c2",
    brightWhite: "#ffffff",
};
