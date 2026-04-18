/// SSE (Server-Sent Events) endpoint handler.
///
/// Clients connect to `GET /event-stream?id=<client_id>` and receive a
/// persistent HTTP response with `Content-Type: text/event-stream`.
/// Events are formatted as `data: {json}\n\n` per the SSE specification.
#[cfg(not(target_arch = "wasm32"))]
pub const SSE_ENDPOINT: &str = "/event-stream";

#[cfg(not(target_arch = "wasm32"))]
pub async fn handle_sse(req: http::request::Parts, ctx_h: u32) -> hyper::Response<hyper::Body> {
    use futures::stream::unfold;
    use hyper::Body;
    use mm2_core::mm_ctx::MmArc;
    use serde_json::json;

    let ctx = match MmArc::from_ffi_handle(ctx_h) {
        Ok(ctx) => ctx,
        Err(e) => {
            return hyper::Response::builder()
                .status(500)
                .body(Body::from(format!("No context: {}", e)))
                .unwrap();
        },
    };

    // Extract client_id from query string: ?id=<u64>
    let client_id = req
        .uri
        .query()
        .and_then(|q| {
            q.split('&')
                .find_map(|pair| pair.strip_prefix("id="))
                .and_then(|v| v.parse::<u64>().ok())
        })
        .unwrap_or(0);

    let cors = ctx.event_stream_access_control();
    let manager = ctx.event_stream_manager.clone();
    let handle = manager.new_client(client_id);

    // Build a streaming body: unfold yields SSE-formatted chunks from the event rx.
    let stream = unfold(handle.rx, |mut rx| async move {
        let event = rx.recv().await?;
        let (origin, payload) = event.get();
        let msg = json!({
            "origin": origin,
            "payload": payload,
            "error": event.is_error(),
        });
        let formatted = format!("data: {}\n\n", msg);
        let bytes = hyper::body::Bytes::from(formatted);
        Some((Ok::<_, std::convert::Infallible>(bytes), rx))
    });

    let body = Body::wrap_stream(stream);

    hyper::Response::builder()
        .status(200)
        .header("Content-Type", "text/event-stream")
        .header("Cache-Control", "no-cache")
        .header("Connection", "keep-alive")
        .header("Access-Control-Allow-Origin", cors)
        .body(body)
        .unwrap()
}
