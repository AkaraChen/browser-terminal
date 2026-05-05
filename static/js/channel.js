export function ensureChannelId() {
    const params = new URLSearchParams(window.location.search);
    let channelId = params.get("channel") || params.get("channel_id");

    if (!channelId) {
        channelId = crypto.randomUUID();
        const nextUrl = new URL(window.location.href);
        nextUrl.searchParams.set("channel", channelId);
        window.history.replaceState(null, "", nextUrl);
    }

    return channelId;
}

export function newChannelUrl() {
    const nextUrl = new URL(window.location.href);
    nextUrl.searchParams.set("channel", crypto.randomUUID());
    return nextUrl;
}
