use std::fs;
use std::io::{BufRead, BufReader, Write};
use std::os::unix::net::{UnixListener, UnixStream};
use std::sync::mpsc::SyncSender;
use std::sync::{Arc, Mutex};
use std::thread;

use anyhow::Result;
use calloop::channel::Sender;
use log::{info, warn};
use unixkey_core::{DaemonStatus, IpcRequest, IpcResponse, runtime_socket_path};

pub struct IpcCommand {
    pub req: IpcRequest,
    pub reply_tx: SyncSender<IpcResponse>,
    pub subscriber_stream: Option<UnixStream>,
}

#[derive(Clone, Default)]
pub struct SubscriberRegistry {
    subscribers: Arc<Mutex<Vec<UnixStream>>>,
}

impl SubscriberRegistry {
    pub fn add(&self, stream: UnixStream) {
        if let Ok(mut guard) = self.subscribers.lock() {
            guard.push(stream);
        }
    }

    pub fn broadcast_status(&self, status: &DaemonStatus) {
        let resp = IpcResponse::Status(Box::new(status.clone()));
        let Ok(mut line) = serde_json::to_string(&resp) else {
            return;
        };
        line.push('\n');
        let bytes = line.as_bytes();

        if let Ok(mut guard) = self.subscribers.lock() {
            guard.retain_mut(|stream| stream.write_all(bytes).is_ok() && stream.flush().is_ok());
        }
    }
}

pub fn spawn_ipc_server<F, E>(tx: Sender<E>, map_cmd: F) -> Result<()>
where
    F: Fn(IpcCommand) -> E + Send + Sync + 'static,
    E: Send + 'static,
{
    let sock_path = runtime_socket_path();
    if sock_path.exists() {
        let _ = fs::remove_file(&sock_path);
    }

    let listener = UnixListener::bind(&sock_path)?;
    info!("Listening for IPC control connections at {}", sock_path.display());

    let map_arc = Arc::new(map_cmd);

    thread::Builder::new()
        .name("unixkey-ipc-server".to_string())
        .spawn(move || {
            for incoming in listener.incoming() {
                match incoming {
                    Ok(stream) => {
                        let tx_clone = tx.clone();
                        let map_clone = Arc::clone(&map_arc);
                        thread::spawn(move || {
                            handle_client(stream, tx_clone, map_clone);
                        });
                    }
                    Err(e) => {
                        warn!("IPC accept error: {e}");
                    }
                }
            }
        })?;

    Ok(())
}

fn handle_client<F, E>(stream: UnixStream, tx: Sender<E>, map_cmd: Arc<F>)
where
    F: Fn(IpcCommand) -> E + Send + Sync + 'static,
    E: Send + 'static,
{
    let Ok(write_clone) = stream.try_clone() else {
        return;
    };
    let mut writer = write_clone;
    let mut reader = BufReader::new(stream);
    let mut line = String::new();

    if reader.read_line(&mut line).is_err() || line.trim().is_empty() {
        return;
    }

    let req: IpcRequest = match serde_json::from_str(line.trim()) {
        Ok(r) => r,
        Err(e) => {
            let err_resp = IpcResponse::Error(format!("Invalid JSON request: {e}"));
            if let Ok(mut s) = serde_json::to_string(&err_resp) {
                s.push('\n');
                let _ = writer.write_all(s.as_bytes());
            }
            return;
        }
    };

    let (reply_tx, reply_rx) = std::sync::mpsc::sync_channel(1);
    let subscriber_stream = if matches!(req, IpcRequest::Subscribe) {
        writer.try_clone().ok()
    } else {
        None
    };

    let cmd = IpcCommand {
        req,
        reply_tx,
        subscriber_stream,
    };

    if tx.send(map_cmd(cmd)).is_err() {
        return;
    }

    if let Ok(resp) = reply_rx.recv()
        && let Ok(mut payload) = serde_json::to_string(&resp)
    {
        payload.push('\n');
        let _ = writer.write_all(payload.as_bytes());
        let _ = writer.flush();
    }
}
