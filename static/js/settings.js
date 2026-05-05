import { DEFAULT_SETTINGS, SETTINGS_KEY, TERMINAL_FONTS } from "./config.js";

export function readSettings() {
    try {
        const raw = localStorage.getItem(SETTINGS_KEY);
        if (!raw) return { ...DEFAULT_SETTINGS };

        return normalizeSettings(JSON.parse(raw));
    } catch {
        return { ...DEFAULT_SETTINGS };
    }
}

export function normalizeSettings(value) {
    const fontFamily = TERMINAL_FONTS.some(
        (font) => font.family === value?.fontFamily,
    )
        ? value.fontFamily
        : DEFAULT_SETTINGS.fontFamily;
    const fontSize = Number.parseInt(value?.fontSize, 10);

    return {
        fontFamily,
        fontSize: Number.isFinite(fontSize)
            ? Math.min(28, Math.max(10, fontSize))
            : DEFAULT_SETTINGS.fontSize,
    };
}

export function persistSettings(nextSettings) {
    const settings = normalizeSettings(nextSettings);
    localStorage.setItem(SETTINGS_KEY, JSON.stringify(settings));
    return settings;
}

export async function loadTerminalFont(fontFamily, webFontsAddon) {
    const font = TERMINAL_FONTS.find((font) => font.family === fontFamily);
    if (!font?.face) return true;

    try {
        await webFontsAddon.loadFonts([font.face]);
        return true;
    } catch {
        return false;
    }
}
