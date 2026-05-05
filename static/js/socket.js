export function createTerminalSocket({
    channelId,
    term,
    setStatus,
    sendResize,
}) {
    const wsProtocol = window.location.protocol === "https:" ? "wss:" : "ws:";
    const socket = new WebSocket(
        `${wsProtocol}//${window.location.host}/ws/${encodeURIComponent(channelId)}`,
    );
    socket.binaryType = "arraybuffer";

    socket.addEventListener("open", () => {
        setStatus("connected");
        sendResize();
        term.focus();
    });

    socket.addEventListener("message", (event) => {
        if (event.data instanceof ArrayBuffer) {
            term.write(new Uint8Array(event.data));
            return;
        }

        if (event.data instanceof Blob) {
            event.data
                .arrayBuffer()
                .then((buffer) => term.write(new Uint8Array(buffer)));
            return;
        }

        term.write(event.data);
    });

    socket.addEventListener("close", () => {
        setStatus("disconnected");
        term.writeln("");
        term.writeln("[disconnected]");
    });

    socket.addEventListener("error", () => {
        setStatus("disconnected");
    });

    return socket;
}

export function sendSocketData(socket, encoder, data) {
    if (socket?.readyState === WebSocket.OPEN) {
        socket.send(encoder.encode(data));
    }
}
