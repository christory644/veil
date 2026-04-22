//! Per-connection I/O handler.
#![allow(dead_code)]
//!
//! Reads newline-delimited JSON from a client, dispatches each request through
//! the `Dispatcher`, and writes the response back. One instance runs per client
//! connection.

use std::sync::Arc;

use tokio::io::{AsyncBufReadExt, AsyncWriteExt};

use crate::dispatcher::Dispatcher;
use crate::rpc::{ErrorResponse, Request, Response, RpcOutcome, INVALID_REQUEST};

/// Handle a single client connection to completion.
///
/// Reads newline-delimited JSON from `reader`, dispatches each request
/// through `dispatcher`, and writes responses back to `writer`.
/// Returns when the client disconnects or `shutdown` is triggered.
///
/// Protocol notes:
/// - Parse failure → write a `parse_error` response and continue (connection stays open).
/// - Valid JSON that is not a valid `Request` → write an `invalid_request` response.
/// - Valid request → dispatch, write response (or nothing for notifications).
/// - All responses are compact JSON followed by `\n`.
pub async fn handle_connection(
    reader: tokio::io::BufReader<tokio::net::unix::OwnedReadHalf>,
    mut writer: tokio::io::BufWriter<tokio::net::unix::OwnedWriteHalf>,
    dispatcher: Arc<Dispatcher>,
    mut shutdown: veil_core::lifecycle::ShutdownHandle,
) {
    let mut lines = reader.lines();

    loop {
        let line = tokio::select! {
            () = shutdown.wait() => break,
            result = lines.next_line() => result,
        };

        let raw = match line {
            Ok(Some(l)) => l,
            Ok(None) => break, // EOF — client disconnected cleanly
            Err(e) => {
                tracing::error!("connection I/O error: {e}");
                break;
            }
        };

        // Step 1: parse line as serde_json::Value
        let json_value: serde_json::Value = if let Ok(v) = serde_json::from_str(&raw) {
            v
        } else {
            let err = ErrorResponse::parse_error();
            write_json(&mut writer, &err).await;
            continue;
        };

        // Step 2: try to deserialize as a Request
        let request: Request = if let Ok(r) = serde_json::from_value(json_value.clone()) {
            r
        } else {
            let id = json_value.get("id").cloned().unwrap_or(serde_json::Value::Null);
            let err = ErrorResponse::new(id, INVALID_REQUEST, "Invalid Request");
            write_json(&mut writer, &err).await;
            continue;
        };

        // Capture the request id before consuming the request in dispatch.
        let id = request.id.clone().unwrap_or(serde_json::Value::Null);

        // Step 3: dispatch and write response
        match dispatcher.dispatch(request).await {
            Some(RpcOutcome::Ok(result)) => {
                let resp = Response::ok(id, result);
                write_json(&mut writer, &resp).await;
            }
            Some(RpcOutcome::Err(err)) => {
                write_json(&mut writer, &err).await;
            }
            None => {
                // Notification — no response written
            }
        }
    }
}

/// Serialize `value` as compact JSON, append `\n`, write to `writer`, and flush.
async fn write_json<T: serde::Serialize>(
    writer: &mut tokio::io::BufWriter<tokio::net::unix::OwnedWriteHalf>,
    value: &T,
) {
    match serde_json::to_string(value) {
        Ok(json) => {
            let line = format!("{json}\n");
            if let Err(e) = writer.write_all(line.as_bytes()).await {
                tracing::error!("failed to write response: {e}");
                return;
            }
            if let Err(e) = writer.flush().await {
                tracing::error!("failed to flush writer: {e}");
            }
        }
        Err(e) => {
            tracing::error!("failed to serialize response: {e}");
        }
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// Tests
// ─────────────────────────────────────────────────────────────────────────────

#[cfg(test)]
#[allow(clippy::items_after_statements)]
mod tests {
    use super::*;
    use crate::dispatcher::Dispatcher;
    use crate::rpc::{INVALID_REQUEST, PARSE_ERROR};
    use serde_json::json;
    use std::sync::Arc;
    use tokio::io::{AsyncBufReadExt, AsyncWriteExt};
    use veil_core::lifecycle::ShutdownSignal;
    use veil_core::state::AppState;

    /// Build a `Dispatcher` backed by a fresh `AppState`.
    fn make_dispatcher() -> Arc<Dispatcher> {
        let state = Arc::new(tokio::sync::Mutex::new(AppState::new()));
        Arc::new(Dispatcher::new(state))
    }

    /// Run a full request/response exchange using a `UnixStream` pair.
    ///
    /// For each request line, writes it to the server and reads back one response
    /// line. Returns all responses in order.
    async fn exchange(requests: &[&str], dispatcher: Arc<Dispatcher>) -> Vec<String> {
        use tokio::net::UnixStream;
        let (client_stream, server_stream) = UnixStream::pair().expect("unix pair");
        let (server_read, server_write) = server_stream.into_split();
        let server_reader = tokio::io::BufReader::new(server_read);
        let server_writer = tokio::io::BufWriter::new(server_write);

        let signal = ShutdownSignal::new();
        let shutdown = signal.handle();

        tokio::spawn(async move {
            handle_connection(server_reader, server_writer, dispatcher, shutdown).await;
        });

        let (client_read, client_write) = client_stream.into_split();
        let mut writer = tokio::io::BufWriter::new(client_write);
        let mut reader = tokio::io::BufReader::new(client_read);

        let mut responses = Vec::new();
        for req in requests {
            let line = format!("{req}\n");
            writer.write_all(line.as_bytes()).await.expect("write request");
            writer.flush().await.expect("flush");

            let mut resp_line = String::new();
            reader.read_line(&mut resp_line).await.expect("read response");
            responses.push(resp_line.trim_end_matches('\n').to_string());
        }
        responses
    }

    // ── Unit 6: Connection handler ────────────────────────────────────────────

    #[tokio::test]
    async fn invalid_json_returns_parse_error() {
        let dispatcher = make_dispatcher();
        let responses = exchange(&["not json"], dispatcher).await;
        assert_eq!(responses.len(), 1);
        let v: serde_json::Value = serde_json::from_str(&responses[0]).expect("parse response");
        assert_eq!(v["error"]["code"], PARSE_ERROR);
    }

    #[tokio::test]
    async fn valid_json_non_request_returns_invalid_request() {
        let dispatcher = make_dispatcher();
        let responses = exchange(&["42"], dispatcher).await;
        assert_eq!(responses.len(), 1);
        let v: serde_json::Value = serde_json::from_str(&responses[0]).expect("parse response");
        assert_eq!(v["error"]["code"], INVALID_REQUEST);
    }

    #[tokio::test]
    async fn workspace_list_request_returns_result() {
        let dispatcher = make_dispatcher();
        let req = json!({"jsonrpc":"2.0","method":"workspace.list","id":1}).to_string();
        let responses = exchange(&[&req], dispatcher).await;
        assert_eq!(responses.len(), 1);
        let v: serde_json::Value = serde_json::from_str(&responses[0]).expect("parse response");
        assert!(v.get("result").is_some(), "response should have 'result' key, got: {v}");
    }

    #[tokio::test]
    async fn notification_request_produces_no_response() {
        use tokio::net::UnixStream;
        let (client_stream, server_stream) = UnixStream::pair().expect("unix pair");
        let (server_read, server_write) = server_stream.into_split();
        let server_reader = tokio::io::BufReader::new(server_read);
        let server_writer = tokio::io::BufWriter::new(server_write);

        let dispatcher = make_dispatcher();
        let signal = ShutdownSignal::new();
        let shutdown = signal.handle();

        tokio::spawn(async move {
            handle_connection(server_reader, server_writer, dispatcher, shutdown).await;
        });

        let (client_read, client_write) = client_stream.into_split();
        let mut writer = tokio::io::BufWriter::new(client_write);
        let mut reader = tokio::io::BufReader::new(client_read);

        // Send a notification (no id field).
        let notif = json!({"jsonrpc":"2.0","method":"workspace.list"}).to_string() + "\n";
        writer.write_all(notif.as_bytes()).await.expect("write notification");
        writer.flush().await.expect("flush");

        // Then send a regular request so we know the server is still alive.
        let req = json!({"jsonrpc":"2.0","method":"workspace.list","id":1}).to_string() + "\n";
        writer.write_all(req.as_bytes()).await.expect("write request");
        writer.flush().await.expect("flush");

        // We should receive exactly ONE response (for the regular request).
        let mut line = String::new();
        reader.read_line(&mut line).await.expect("read response");
        let v: serde_json::Value = serde_json::from_str(line.trim()).expect("parse response");
        assert_eq!(v["id"], json!(1), "response should match the regular request id");
    }

    #[tokio::test]
    async fn multiple_requests_all_handled() {
        let dispatcher = make_dispatcher();
        let req1 = json!({"jsonrpc":"2.0","method":"workspace.list","id":1}).to_string();
        let req2 = json!({"jsonrpc":"2.0","method":"workspace.list","id":2}).to_string();
        let req3 = json!({"jsonrpc":"2.0","method":"workspace.list","id":3}).to_string();
        let responses = exchange(&[&req1, &req2, &req3], dispatcher).await;
        assert_eq!(responses.len(), 3);
        let ids: Vec<i64> = responses
            .iter()
            .map(|r| {
                let v: serde_json::Value = serde_json::from_str(r).expect("parse");
                v["id"].as_i64().expect("id")
            })
            .collect();
        assert_eq!(ids, vec![1, 2, 3]);
    }

    #[tokio::test]
    async fn client_disconnect_returns_cleanly() {
        use tokio::net::UnixStream;
        let (client_stream, server_stream) = UnixStream::pair().expect("unix pair");
        let (server_read, server_write) = server_stream.into_split();
        let server_reader = tokio::io::BufReader::new(server_read);
        let server_writer = tokio::io::BufWriter::new(server_write);

        let dispatcher = make_dispatcher();
        let signal = ShutdownSignal::new();
        let shutdown = signal.handle();

        let handle = tokio::spawn(async move {
            handle_connection(server_reader, server_writer, dispatcher, shutdown).await;
        });

        // Drop the client — this closes the connection.
        drop(client_stream);

        // The connection handler should return cleanly (no panic).
        tokio::time::timeout(std::time::Duration::from_secs(2), handle)
            .await
            .expect("timeout — handle_connection did not return after client disconnect")
            .expect("task panicked");
    }

    #[tokio::test]
    async fn response_is_newline_terminated() {
        let dispatcher = make_dispatcher();
        use tokio::net::UnixStream;
        let (client_stream, server_stream) = UnixStream::pair().expect("unix pair");
        let (server_read, server_write) = server_stream.into_split();
        let server_reader = tokio::io::BufReader::new(server_read);
        let server_writer = tokio::io::BufWriter::new(server_write);
        let signal = ShutdownSignal::new();
        let shutdown = signal.handle();

        tokio::spawn(async move {
            handle_connection(server_reader, server_writer, dispatcher, shutdown).await;
        });

        let (client_read, client_write) = client_stream.into_split();
        let mut writer = tokio::io::BufWriter::new(client_write);

        let req = json!({"jsonrpc":"2.0","method":"workspace.list","id":1}).to_string() + "\n";
        writer.write_all(req.as_bytes()).await.expect("write");
        writer.flush().await.expect("flush");

        let mut client_reader = tokio::io::BufReader::new(client_read);
        let mut response_line = String::new();
        client_reader.read_line(&mut response_line).await.expect("read");
        assert!(
            response_line.ends_with('\n'),
            "response should end with newline, got: {response_line:?}"
        );
    }

    #[tokio::test]
    async fn response_preserves_request_id() {
        let dispatcher = make_dispatcher();
        let req = json!({"jsonrpc":"2.0","method":"workspace.list","id":"my-id-42"}).to_string();
        let responses = exchange(&[&req], dispatcher).await;
        let v: serde_json::Value = serde_json::from_str(&responses[0]).expect("parse response");
        assert_eq!(v["id"], json!("my-id-42"), "response id should match request id");
    }
}
