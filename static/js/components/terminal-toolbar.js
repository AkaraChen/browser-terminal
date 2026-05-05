export function terminalToolbar() {
    return {
        openSettings() {
            window.dispatchEvent(new CustomEvent("settings-open"));
        },

        openNewTab() {
            this.$store.terminal.openNewTab();
        },
    };
}
