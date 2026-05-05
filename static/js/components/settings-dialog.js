export function settingsDialog() {
    return {
        open() {
            this.$store.terminal.syncSettingsForm();
            this.$refs.dialog.showModal();
        },

        updateFontFamily(fontFamily) {
            this.$store.terminal.updateFontFamily(fontFamily);
        },

        updateFontSize(fontSize) {
            this.$store.terminal.updateFontSize(fontSize);
        },

        updateFontSizeInput(fontSize) {
            this.$store.terminal.updateFontSizeInput(fontSize);
        },

        syncSettingsForm() {
            this.$store.terminal.syncSettingsForm();
        },
    };
}
