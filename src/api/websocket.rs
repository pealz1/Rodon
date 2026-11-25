//! Loopback WebSocket compatibility for Evanovar RAM's text commands.
use crate::services::Services;
use serde_json::{Value, json};
use std::{
    io,
    net::{TcpListener, TcpStream},
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, AtomicUsize, Ordering},
    },
    time::{Duration, Instant},
};
use tungstenite::{
    Message,
    handshake::server::{ErrorResponse, Request, Response},
};

pub struct Server {
    stop: Arc<AtomicBool>,
    listener: Option<std::thread::JoinHandle<()>>,
}
impl Drop for Server {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Release);
        if let Some(thread) = self.listener.take() {
            let _ = thread.join();
        }
    }
}

pub fn start(s: &Arc<Services>, port: u16) -> io::Result<Server> {
    let socket = TcpListener::bind(("127.0.0.1", port))?;
    socket.set_nonblocking(true)?;
    let stop = Arc::new(AtomicBool::new(false));
    let quit = stop.clone();
    let services = Arc::downgrade(s);
    let listener = std::thread::Builder::new().name("websocket".into()).spawn(move || {
        let failures = Arc::new(Mutex::new(Vec::<Instant>::new()));
        let active = Arc::new(AtomicUsize::new(0));
        while !quit.load(Ordering::Acquire) {
            match socket.accept() {
                Ok((stream, _)) => {
                    if active.load(Ordering::Acquire) >= 8 {
                        continue;
                    }
                    let Some(s) = services.upgrade() else { break };
                    let stop = quit.clone();
                    let slots = active.clone();
                    let failures = failures.clone();
                    active.fetch_add(1, Ordering::AcqRel);
                    std::thread::spawn(move || {
                        serve(stream, &s, &stop, &failures);
                        slots.fetch_sub(1, Ordering::AcqRel);
                    });
                }
                Err(e) if e.kind() == io::ErrorKind::WouldBlock => std::thread::sleep(Duration::from_millis(120)),
                Err(_) => break,
            }
        }
    })?;
    Ok(Server { stop, listener: Some(listener) })
}
#[allow(clippy::result_large_err)] // Tungstenite requires the full HTTP error response in its handshake callback.
fn serve(stream: TcpStream, s: &Arc<Services>, stop: &AtomicBool, failures: &Mutex<Vec<Instant>>) {
    let _ = stream.set_read_timeout(Some(Duration::from_secs(2)));
    let _ = stream.set_write_timeout(Some(Duration::from_secs(2)));
    let cfg = tungstenite::protocol::WebSocketConfig::default().max_message_size(Some(64 * 1024)).max_frame_size(Some(64 * 1024));
    let Ok(mut ws) = tungstenite::accept_hdr_with_config(
        stream,
        |req: &Request, res: Response| -> Result<Response, ErrorResponse> {
            if req.headers().contains_key("origin") {
                return Err(tungstenite::http::Response::builder().status(403).body(Some("Browser origins are refused".into())).unwrap());
            }
            Ok(res)
        },
        Some(cfg),
    ) else {
        return;
    };
    while !stop.load(Ordering::Acquire) {
        let text = match ws.read() {
            Ok(Message::Text(t)) => t,
            Ok(Message::Ping(_)) => {
                let _ = ws.flush();
                continue;
            }
            Ok(Message::Close(_)) => break,
            Ok(_) => continue,
            Err(tungstenite::Error::Io(e)) if matches!(e.kind(), io::ErrorKind::WouldBlock | io::ErrorKind::TimedOut) => continue,
            Err(_) => break,
        };
        if stop.load(Ordering::Acquire) {
            break;
        }
        let pw = s.core.secret("websocket_password").unwrap_or_default();
        let mut command = text.trim();
        let required = s.core.settings().websocket_auth;
        let checked = {
            let mut attempts = failures.lock().unwrap_or_else(|e| e.into_inner());
            attempts.retain(|at| at.elapsed() < Duration::from_secs(60));
            if attempts.len() >= 10 {
                false
            } else if required {
                let pair = command.split_once('|').and_then(|(auth, cmd)| auth.trim().strip_prefix("AUTH ").map(|p| (p, cmd.trim())));
                if let Some((p, cmd)) = pair
                    && !pw.is_empty()
                    && super::http::constant_time_eq(p, &pw)
                {
                    command = cmd;
                    true
                } else {
                    attempts.push(Instant::now());
                    false
                }
            } else {
                true
            }
        };
        let response = if checked {
            match dispatch(s, command) {
                Ok(result) if command.eq_ignore_ascii_case("GetStatus") => json!({"ok":true,"action":"GetStatus","result":result}),
                Ok(result) => json!({"ok":true,"result":result}),
                Err(e) => json!({"ok":false,"error":e}),
            }
        } else {
            json!({"ok":false,"error":"Authentication failed"})
        };
        if ws.send(Message::Text(response.to_string().into())).is_err() {
            break;
        }
    }
}
fn dispatch(s: &Arc<Services>, command: &str) -> Result<Value, String> {
    let mut words = command.split_whitespace();
    let verb = words.next().unwrap_or("").to_ascii_lowercase();
    let account = words.next().unwrap_or("");
    let arg = words.next().unwrap_or("");
    match verb.as_str() {
        "ping" => Ok(json!("Pong")),
        "accountlist" => {
            let mut names: Vec<_> = s.core.accounts().into_iter().map(|a| a.username).collect();
            names.sort();
            Ok(json!({"action":"AccountList","accounts":names,"count":names.len()}))
        }
        "getstatus" => {
            let mut instances = s.live.instances();
            instances.sort_unstable_by_key(|i| i.pid);
            Ok(Value::Array(
                instances
                    .into_iter()
                    .map(|i| {
                        if i.user_id == 0 {
                            json!({"pid":i.pid, "username":null})
                        } else {
                            json!({"pid":i.pid, "username":(!i.username.is_empty()).then_some(i.username),"user_id":i.user_id})
                        }
                    })
                    .collect(),
            ))
        }
        "launch" if !account.is_empty() && !arg.is_empty() => {
            let req = crate::roblox::launch::LaunchRequest {
                place_id: arg.to_owned(),
                private_server: words.next().unwrap_or("").into(),
                job_id: words.next().unwrap_or("").into(),
            };
            s.core.launch(account, &req).map_err(|e| e.message)?;
            Ok(json!({"action":"Launch","account":account,"place_id":arg,"private_server":req.private_server,"job_id":req.job_id}))
        }
        "joinuser" if !arg.is_empty() => {
            let result = s.core.join_user(&[account.into()], arg).map_err(|e| e.message)?;
            if let Some((_, e)) = result.failures.first() {
                return Err(e.message.clone());
            }
            Ok(json!({"action":"JoinUser","account":account,"target_user":arg}))
        }
        "autorejoin" if !arg.is_empty() => {
            let key = s.core.account(arg).ok_or("Unknown account")?.key();
            match account.to_ascii_lowercase().as_str() {
                "start" => {
                    let cfg =
                        s.core.snapshot().auto_rejoin.iter().find(|c| c.account == key).cloned().ok_or("Configure Auto-Rejoin first")?;
                    s.rejoin.start(s, cfg);
                }
                "stop" => s.rejoin.stop(&key),
                _ => return Err("Choose start or stop".into()),
            }
            Ok(json!({"action":"AutoRejoin","account":arg,"mode":account}))
        }
        "add" if !account.is_empty() => {
            let mut pending = s.pending_import.lock().unwrap_or_else(|p| p.into_inner());
            if pending.is_some() {
                return Err("Another import is awaiting review".into());
            }
            *pending = Some(command.split_once(char::is_whitespace).map(|v| v.1.trim()).unwrap_or("").to_owned());
            s.request_repaint();
            Ok(json!({"action":"Add","pending_review":true}))
        }
        _ => Err("Unknown command or missing arguments".into()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn legacy_commands_require_the_configured_password() {
        let (_dir, core) = crate::core::test_core();
        core.set_secret("websocket_password", "fixture-only").unwrap();
        let s = Services::new(core);
        let listener = TcpListener::bind(("127.0.0.1", 0)).unwrap();
        let port = listener.local_addr().unwrap().port();
        let handle = std::thread::spawn(move || {
            let (stream, _) = listener.accept().unwrap();
            serve(stream, &s, &AtomicBool::new(false), &Mutex::new(vec![]));
        });
        let (mut ws, _) = tungstenite::connect(format!("ws://127.0.0.1:{port}")).unwrap();
        let mut call = |message: &str| {
            ws.send(Message::Text(message.into())).unwrap();
            serde_json::from_str::<Value>(&ws.read().unwrap().into_text().unwrap()).unwrap()
        };
        assert_eq!(call("Ping")["ok"], false);
        assert_eq!(call("AUTH fixture-only | Ping")["result"], "Pong");
        assert_eq!(call("AUTH fixture-only | GetStatus"), json!({"ok":true,"action":"GetStatus","result":[]}));
        assert_eq!(call("AUTH fixture-only | AccountList")["result"]["count"], 0);
        ws.close(None).unwrap();
        drop(ws);
        handle.join().unwrap();
    }
}
