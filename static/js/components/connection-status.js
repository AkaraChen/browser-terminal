export function connectionStatus() {
    return {
        get statusClass() {
            return this.$store.terminal.statusClass;
        },

        get label() {
            return this.$store.terminal.statusLabel;
        },
    };
}
