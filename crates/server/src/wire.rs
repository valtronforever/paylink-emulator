use crate::Shared;
use anyhow::{Context, Result, bail};
use paylink_core::{Delivery, catalog::error_definition};
use serde_json::{Value, json};
use std::{collections::BTreeMap, net::SocketAddr, time::Duration};
use tokio::{
    io::{AsyncReadExt, AsyncWriteExt},
    net::{TcpListener, TcpStream},
    task::JoinSet,
};

pub async fn serve(initial: TcpListener, addr: SocketAddr, s: Shared) {
    let mut listener = Some(initial);
    let mut online = s.transport.subscribe();
    let mut stop = s.stop.subscribe();
    let mut reset = s.reset_connections.subscribe();
    let mut connections = JoinSet::new();
    if !*online.borrow_and_update() {
        listener = None;
    }
    let generation = *reset.borrow_and_update();
    {
        let mut data = s.data.lock().await;
        data.listener_online = listener.is_some();
        data.listener_generation = generation;
    }
    if *stop.borrow() {
        return;
    }
    loop {
        tokio::select! {
            _=stop.changed()=>break,
            _=reset.changed()=> {
                connections.abort_all();
                while connections.join_next().await.is_some() {}
                let generation=*reset.borrow_and_update();
                s.data.lock().await.listener_generation=generation;
            },
            _=online.changed()=> {
                if !*online.borrow_and_update() {
                    listener=None; connections.abort_all();
                    let mut d=s.data.lock().await; d.listener_online=false; d.listener_error=None;
                } else if listener.is_none() {
                    match TcpListener::bind(addr).await {
                        Ok(l)=> {listener=Some(l);let mut d=s.data.lock().await;d.listener_online=true;d.listener_error=None;},
                        Err(e)=> {let mut d=s.data.lock().await;d.listener_online=false;d.listener_error=Some(e.to_string());}
                    }
                }
            },
            accepted=async { match &listener {Some(l)=>l.accept().await,None=>std::future::pending().await} } => {
                if let Ok((socket,_))=accepted {
                    let shared=s.clone();
                    let generation=s.data.lock().await.engine.generation;
                    connections.spawn(async move { let _=connection(socket,shared,generation).await; });
                }
            },
            _=connections.join_next(),if !connections.is_empty()=>{}
        }
    }
    connections.abort_all();
    while connections.join_next().await.is_some() {}
}
struct Request {
    method: String,
    path: String,
    headers: BTreeMap<String, String>,
    body: Vec<u8>,
}
async fn parse(socket: &mut TcpStream) -> Result<Request> {
    let mut bytes = Vec::new();
    let end = loop {
        if let Some(i) = bytes.windows(4).position(|w| w == b"\r\n\r\n") {
            break i + 4;
        }
        if bytes.len() > 16 * 1024 {
            bail!("headers too large");
        }
        let mut buf = [0; 1024];
        let n = socket.read(&mut buf).await?;
        if n == 0 {
            bail!("closed");
        }
        bytes.extend_from_slice(&buf[..n]);
    };
    let head = std::str::from_utf8(&bytes[..end])?;
    let mut lines = head.split("\r\n");
    let mut first = lines
        .next()
        .context("missing request line")?
        .split_whitespace();
    let method = first.next().context("method missing")?.to_owned();
    let path = first
        .next()
        .context("path missing")?
        .split('?')
        .next()
        .unwrap()
        .to_owned();
    if first.next() != Some("HTTP/1.1") {
        bail!("HTTP/1.1 required");
    }
    let mut headers = BTreeMap::new();
    for line in lines.filter(|l| !l.is_empty()) {
        let (key, value) = line.split_once(':').context("invalid header")?;
        if headers
            .insert(key.to_ascii_lowercase(), value.trim().to_owned())
            .is_some()
        {
            bail!("duplicate header");
        }
    }
    if headers.contains_key("transfer-encoding") {
        bail!("chunked requests unsupported");
    }
    let length: usize = headers
        .get("content-length")
        .map(|v| v.parse())
        .transpose()?
        .unwrap_or(0);
    if length > 64 * 1024 {
        bail!("body too large");
    }
    let mut body = bytes[end..].to_vec();
    while body.len() < length {
        let mut buf = [0; 4096];
        let n = socket.read(&mut buf).await?;
        if n == 0 {
            bail!("incomplete body");
        }
        body.extend_from_slice(&buf[..n]);
    }
    body.truncate(length);
    Ok(Request {
        method,
        path,
        headers,
        body,
    })
}
async fn respond(
    socket: &mut TcpStream,
    status: u16,
    body: &[u8],
    origin: Option<&str>,
    partial: bool,
    content_type: &str,
) -> Result<()> {
    let reason = match status {
        200 => "OK",
        204 => "No Content",
        400 => "Bad Request",
        403 => "Forbidden",
        404 => "Not Found",
        405 => "Method Not Allowed",
        409 => "Conflict",
        501 => "Not Implemented",
        _ => "Internal Server Error",
    };
    let cors=origin.map(|o|format!("Access-Control-Allow-Origin: {o}\r\nVary: Origin\r\nAccess-Control-Allow-Methods: GET, POST, OPTIONS\r\nAccess-Control-Allow-Headers: Content-Type\r\nAccess-Control-Allow-Private-Network: true\r\n")).unwrap_or_default();
    let head = format!(
        "HTTP/1.1 {status} {reason}\r\nContent-Type: {content_type}\r\nContent-Length: {}\r\nConnection: close\r\nCache-Control: no-store\r\n{cors}\r\n",
        body.len()
    );
    socket.write_all(head.as_bytes()).await?;
    socket
        .write_all(if partial {
            &body[..body.len() / 2]
        } else {
            body
        })
        .await?;
    socket.shutdown().await?;
    Ok(())
}
async fn json_response(
    socket: &mut TcpStream,
    status: u16,
    value: Value,
    origin: Option<&str>,
) -> Result<()> {
    respond(
        socket,
        status,
        &serde_json::to_vec(&value)?,
        origin,
        false,
        "application/json",
    )
    .await
}
fn wire_error(code: &str, message: &str) -> Value {
    if code == "terminal_busy" {
        return json!({"success":false,"code":9009,"description":"Device is busy",
            "error":"Пристрій вже зайнятий виконанням команди. Потрібно зачекати декілька секунд та повторити спробу."});
    }
    json!({"success":false,"error":error_definition(code).map(|e|e.message).unwrap_or(message),"code":0,"result":null})
}
async fn connection(mut socket: TcpStream, s: Shared, generation: u64) -> Result<()> {
    let request = match tokio::time::timeout(Duration::from_secs(10), parse(&mut socket)).await {
        Ok(Ok(r)) => r,
        _ => {
            return json_response(
                &mut socket,
                400,
                wire_error("invalid_request", "Invalid request"),
                None,
            )
            .await;
        }
    };
    let origin = request.headers.get("origin").map(String::as_str);
    if let Some(origin) = origin
        && !s.config.allowed_origins.iter().any(|v| v == origin)
    {
        return json_response(
            &mut socket,
            403,
            wire_error("origin_denied", "Origin denied"),
            None,
        )
        .await;
    }
    if request.method == "OPTIONS" {
        return respond(&mut socket, 204, b"", origin, false, "application/json").await;
    }
    let path = request.path.trim_end_matches('/');
    if request.method == "GET" && matches!(path, "/api/devices" | "/api/pos/devices") {
        let d = s.data.lock().await;
        let devices = d
            .engine
            .devices
            .values()
            .map(|d| json!({"id":d.id,"device_id":d.id,"name":d.name,"merchant":d.merchant}))
            .collect::<Vec<_>>();
        drop(d);
        return json_response(&mut socket, 200, json!(devices), origin).await;
    }
    if request.method == "GET"
        && let Some(id) = path
            .strip_prefix("/api/devices/")
            .or_else(|| path.strip_prefix("/api/pos/devices/"))
    {
        let device = s.data.lock().await.engine.devices.get(id).cloned();
        return match device {
            Some(device)=>json_response(&mut socket,200,json!({"id":device.id,"device_id":device.id,"name":device.name,"merchant":device.merchant}),origin).await,
            None=>json_response(&mut socket,404,json!({"loc":[],"msg":format!("POS terminal not found: Id {id}"),"type":"POS terminal"}),origin).await,
        };
    }
    let parts = path.split('/').collect::<Vec<_>>();
    if parts.len() != 5 || parts[1] != "api" || parts[2] != "pos" {
        return json_response(
            &mut socket,
            404,
            wire_error(
                "unsupported_route",
                "Unsupported route in experimental profile",
            ),
            origin,
        )
        .await;
    }
    let device_id = parts[3];
    let action = parts[4];
    if action == "ping" && request.method == "GET" {
        let d = s.data.lock().await;
        if !d.engine.devices.contains_key(device_id) {
            drop(d);
            return json_response(
                &mut socket,
                404,
                json!({
                    "success": false, "code": 9524,
                    "description": format!("Invalid terminal id > Id {device_id}"),
                    "error": "Недійсний ідентифікатор терміналу."
                }),
                origin,
            )
            .await;
        }
        if d.engine
            .operations
            .values()
            .any(|op| op.device_id == device_id && !op.stage.terminal())
        {
            drop(d);
            return json_response(
                &mut socket,
                400,
                wire_error("terminal_busy", "Device is busy"),
                origin,
            )
            .await;
        }
        let result = match d.engine.devices.get(device_id) {
            Some(device) if device.online && device.setup_error.is_none() => {
                json!({"success":true,"terminal_status":"None","error":"","code":0})
            }
            Some(_) => wire_error("terminal_connection_refused", "Terminal unavailable"),
            None => wire_error("terminal_id_invalid", "Unknown terminal"),
        };
        drop(d);
        return json_response(&mut socket, 200, result, origin).await;
    }
    if action != "purchase" {
        return json_response(
            &mut socket,
            404,
            wire_error(
                "unsupported_route",
                "Unsupported route in experimental profile",
            ),
            origin,
        )
        .await;
    }
    if request.method != "POST" {
        return json_response(
            &mut socket,
            405,
            wire_error("invalid_method", "POST required"),
            origin,
        )
        .await;
    }
    if !request.headers.get("content-type").is_some_and(|v| {
        v.split(';')
            .next()
            .unwrap_or("")
            .trim()
            .eq_ignore_ascii_case("application/json")
    }) {
        return json_response(
            &mut socket,
            400,
            wire_error("invalid_content_type", "application/json required"),
            origin,
        )
        .await;
    }
    let payload: Value = match serde_json::from_slice(&request.body) {
        Ok(p) => p,
        Err(_) => {
            return json_response(
                &mut socket,
                400,
                wire_error("invalid_json", "Malformed JSON"),
                origin,
            )
            .await;
        }
    };
    if payload
        .get("merchant_id")
        .is_some_and(|v| !v.is_null() && !v.is_string())
    {
        return json_response(
            &mut socket,
            400,
            wire_error("invalid_merchant", "merchant_id must be a string"),
            origin,
        )
        .await;
    }
    // Real PayLink also accepts transaction identity and confirmation fields. Until
    // reference calibration establishes their semantics, fail explicitly rather
    // than ignoring them and potentially simulating a duplicate charge.
    if let Some(fields) = payload.as_object() {
        let unsupported = fields
            .keys()
            .filter(|key| !matches!(key.as_str(), "amount" | "merchant_id"))
            .cloned()
            .collect::<Vec<_>>();
        if !unsupported.is_empty() {
            return json_response(
                &mut socket,
                501,
                wire_error(
                    "unsupported_request_fields",
                    &format!(
                        "Unsupported fields in experimental profile: {}",
                        unsupported.join(", ")
                    ),
                ),
                origin,
            )
            .await;
        }
    }
    let Some(amount) = payload.get("amount").and_then(Value::as_u64) else {
        return json_response(
            &mut socket,
            400,
            wire_error("invalid_amount", "amount must be integer minor units"),
            origin,
        )
        .await;
    };
    let started = {
        let mut d = s.data.lock().await;
        if d.engine.generation != generation {
            return Ok(());
        }
        d.engine.start(
            device_id,
            amount,
            payload.get("merchant_id").and_then(Value::as_str),
        )
    };
    s.journal().await?;
    let id = match started {
        Ok(id) => id,
        Err(e) if e.code == "disconnect_before_accept" => return Ok(()),
        Err(e) => {
            return json_response(
                &mut socket,
                if matches!(e.code.as_str(), "invalid_amount" | "terminal_busy") {
                    400
                } else {
                    200
                },
                wire_error(&e.code, &e.message),
                origin,
            )
            .await;
        }
    };
    s.journal().await?;
    if s.data.lock().await.engine.operations[&id].scenario.delivery
        == Delivery::DisconnectAfterAccept
    {
        return Ok(());
    }
    loop {
        let snapshot = {
            let d = s.data.lock().await;
            d.engine
                .operations
                .get(&id)
                .map(|op| (op.clone(), d.engine.now_ms, d.engine.result(&id).ok()))
        };
        let Some((op, now, result)) = snapshot else {
            return Ok(());
        }; // Reset invalidated this connection.
        if op.stage.terminal()
            && now
                >= op
                    .completed_ms
                    .unwrap_or(now)
                    .saturating_add(op.scenario.timing.response_ms)
        {
            match op.scenario.delivery {
                Delivery::DisconnectAfterCommit
                | Delivery::DisconnectBeforeAccept
                | Delivery::DisconnectAfterAccept => return Ok(()),
                Delivery::Hang => {}
                delivery => {
                    let result = match delivery {
                        Delivery::MissingFields => json!({"success":true,"result":{}}),
                        Delivery::UnknownCode => {
                            json!({"success":false,"error":"Synthetic unknown code","code":999999,"result":null})
                        }
                        _ => result.context("completed result missing")?,
                    };
                    let body = if delivery == Delivery::MalformedJson {
                        b"{invalid-json".to_vec()
                    } else {
                        serde_json::to_vec(&result)?
                    };
                    respond(
                        &mut socket,
                        if delivery == Delivery::Http400 {
                            400
                        } else if delivery == Delivery::Http500 {
                            500
                        } else {
                            200
                        },
                        &body,
                        origin,
                        delivery == Delivery::PartialResponse,
                        if delivery == Delivery::WrongContentType {
                            "text/plain"
                        } else {
                            "application/json"
                        },
                    )
                    .await?;
                    if delivery == Delivery::Normal {
                        s.data.lock().await.engine.delivered(&id)?;
                        s.journal().await?;
                    }
                    return Ok(());
                }
            }
        }
        if *s.stop.borrow() {
            return Ok(());
        }
        tokio::time::sleep(Duration::from_millis(5)).await;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{Config, Data};
    use paylink_core::Engine;
    use std::{collections::BTreeMap, sync::Arc};
    use tokio::sync::{Mutex, watch};
    #[tokio::test]
    async fn listener_applies_watch_values_set_before_task_first_poll() {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        let (transport, _) = watch::channel(false);
        let (stop, _) = watch::channel(false);
        let (reset_connections, _) = watch::channel(2);
        let shared = Shared {
            data: Arc::new(Mutex::new(Data {
                engine: Engine::default(),
                scenarios: BTreeMap::new(),
                commands: BTreeMap::new(),
                listener_online: true,
                listener_generation: 1,
                listener_error: None,
            })),
            config: Arc::new(Config::default()),
            transport,
            stop,
            reset_connections,
        };
        let task = tokio::spawn(serve(listener, addr, shared.clone()));
        tokio::time::timeout(Duration::from_secs(1), async {
            loop {
                if shared.data.lock().await.listener_generation == 2 {
                    break;
                }
                tokio::task::yield_now().await;
            }
        })
        .await
        .unwrap();
        assert!(!shared.data.lock().await.listener_online);
        let probe = TcpListener::bind(addr).await.unwrap();
        drop(probe);
        shared.stop.send_replace(true);
        task.await.unwrap();
    }
}
