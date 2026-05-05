export function terminalPane() {
    return {
        init() {
            this.$store.terminal.attachTerminal(this.$refs.terminal);
        },
    };
}
