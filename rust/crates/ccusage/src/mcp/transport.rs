//! MCP transports: stdio (newline-delimited JSON-RPC) and a minimal HTTP/SSE
//! server (Streamable HTTP) suitable for local integration.

use std::{
    io::{self, BufRead, BufReader, Read, Write},
    net::TcpListener,
};

use serde_json::Value;

use crate::{cli::McpArgs, Result};

/// Serves MCP over stdin/stdout, one JSON-RPC request per line.
pub(crate) fn serve_stdio(args: &McpArgs) -> Result<()> {
    let stdin = io::stdin();
    let stdout = io::stdout();
    let mut stdout = stdout.lock();

    for line in stdin.lock().lines() {
        let line = line.map_err(|error| crate::cli_error(format!("read stdin: {error}")))?;
        if line.trim().is_empty() {
            continue;
        }
        let Ok(request) = serde_json::from_str::<Value>(&line) else {
            continue;
        };
        if let Some(response) = super::server::handle_request(&request, args) {
            // A write failure means the client went away; stop serving.
            if writeln!(stdout, "{response}").is_err() || stdout.flush().is_err() {
                break;
            }
        }
    }
    Ok(())
}

/// Serves MCP over a minimal HTTP/SSE transport bound to the given port.
pub(crate) fn serve_http(args: &McpArgs) -> Result<()> {
    let address = format!("127.0.0.1:{}", args.port);
    let listener = TcpListener::bind(&address).map_err(|error| {
        crate::cli_error(format!("failed to bind MCP HTTP server to {address}: {error}"))
    })?;
    eprintln!("MCP server is running on http://localhost:{}", args.port);

    for stream in listener.incoming() {
        let Ok(mut stream) = stream else { continue };
        let _ = handle_http_connection(&mut stream, args);
    }
    Ok(())
}

fn handle_http_connection(stream: &mut std::net::TcpStream, args: &McpArgs) -> io::Result<()> {
    let mut reader = BufReader::new(stream.try_clone()?);
    let (request_line, headers) = read_request_head(&mut reader)?;
    let method = request_line.split_whitespace().next().unwrap_or("");
    let accept = header_value(&headers, "accept").unwrap_or_default();
    let accepts_json = accept
        .split(',')
        .any(|part| {
            part.trim() == "application/json" || part.trim().starts_with("application/json")
        });
    let accepts_sse = accept
        .split(',')
        .any(|part| part.trim().starts_with("text/event-stream"));

    // Only POST with a body that accepts both JSON and SSE is a valid MCP request.
    if method != "POST" || !accepts_json || !accepts_sse {
        let body = serde_json::to_string(&serde_json::json!({
            "jsonrpc": "2.0",
            "error": {
                "code": -32000,
                "message": "Not Acceptable: Client must accept both application/json and text/event-stream",
            },
            "id": null,
        }))
        .unwrap_or_else(|_| "{}".to_string());
        return write_response(stream, "406 Not Acceptable", "application/json", &body);
    }

    let content_length = header_value(&headers, "content-length")
        .and_then(|value| value.parse::<usize>().ok())
        .unwrap_or(0);
    let mut body = vec![0u8; content_length];
    if content_length > 0 {
        reader.read_exact(&mut body)?;
    }
    let body = String::from_utf8_lossy(&body).to_string();

    let Ok(request) = serde_json::from_str::<Value>(&body) else {
        let body = serde_json::to_string(&serde_json::json!({
            "jsonrpc": "2.0",
            "error": {"code": -32700, "message": "Parse error"},
            "id": null,
        }))
        .unwrap_or_else(|_| "{}".to_string());
        return write_response(stream, "400 Bad Request", "application/json", &body);
    };

    let payload = match super::server::handle_request(&request, args) {
        Some(response) => serde_json::to_string(&response).unwrap_or_default(),
        // Notifications get an empty accepted response.
        None => String::new(),
    };
    let sse = if payload.is_empty() {
        String::from(": accepted\n\n")
    } else {
        format!("event: message\ndata: {payload}\n\n")
    };
    write_response(stream, "200 OK", "text/event-stream", &sse)
}

fn read_request_head(reader: &mut BufReader<std::net::TcpStream>) -> io::Result<(String, Vec<(String, String)>)> {
    let mut request_line = String::new();
    reader.read_line(&mut request_line)?;
    let mut headers = Vec::new();
    loop {
        let mut line = String::new();
        let bytes = reader.read_line(&mut line)?;
        if bytes == 0 || line.trim().is_empty() {
            break;
        }
        if let Some((name, value)) = line.split_once(':') {
            headers.push((name.trim().to_ascii_lowercase(), value.trim().to_string()));
        }
    }
    Ok((request_line, headers))
}

fn header_value(headers: &[(String, String)], name: &str) -> Option<String> {
    headers
        .iter()
        .find(|(header_name, _)| header_name == name)
        .map(|(_, value)| value.clone())
}

fn write_response(
    stream: &mut std::net::TcpStream,
    status: &str,
    content_type: &str,
    body: &str,
) -> io::Result<()> {
    let response = format!(
        "HTTP/1.1 {status}\r\nContent-Type: {content_type}\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
        body.len()
    );
    stream.write_all(response.as_bytes())?;
    stream.flush()?;
    Ok(())
}
