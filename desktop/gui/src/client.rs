//! Talking to the daemon.
//!
//! The GUI speaks the *same* newline-delimited JSON control protocol the CLI
//! does, over the same Unix socket in `$XDG_RUNTIME_DIR/anyflow/`, using the
//! very same [`Request`]/[`Response`] types from `anyflow-daemon`. Nothing
//! here is a second, parallel interface: if the socket contract changes, this
//! file stops compiling, which is the point.
//!
//! # Why it polls
//!
//! The control socket is request/response — it has no way to push a change.
//! `desktop/daemon/src/control.rs` says as much, and notes that a GUI would
//! eventually want D-Bus so it could receive signals instead. Until that
//! exists, this refreshes on a timer, and the interval is chosen to be cheap
//! rather than instant: everything it asks for is metadata the daemon already
//! holds in memory.
//!
//! # Threading
//!
//! GTK is single-threaded and tokio is not. A runtime runs on its own thread,
//! socket work happens there, and results cross back to the GTK main loop
//! through an `async_channel`. No GTK object is ever touched off the main
//! thread.

use anyflow_control::{Event, Request, Response};
use anyflow_linux::control_socket_path;
use std::sync::OnceLock;
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::net::UnixStream;

fn runtime() -> &'static tokio::runtime::Runtime {
    static RT: OnceLock<tokio::runtime::Runtime> = OnceLock::new();
    RT.get_or_init(|| {
        tokio::runtime::Builder::new_multi_thread()
            .worker_threads(2)
            .enable_all()
            .build()
            .expect("tokio runtime")
    })
}

/// A single request/response exchange with the daemon.
async fn exchange(request: Request) -> anyhow::Result<Response> {
    let path = control_socket_path();
    let stream = UnixStream::connect(&path).await.map_err(|e| {
        anyhow::anyhow!(
            "could not reach the AnyFlow daemon at {}: {e}",
            path.display()
        )
    })?;
    let (read, mut write) = stream.into_split();
    let mut line = serde_json::to_vec(&request)?;
    line.push(b'\n');
    write.write_all(&line).await?;
    write.flush().await?;

    let mut lines = BufReader::new(read).lines();
    let reply = lines
        .next_line()
        .await?
        .ok_or_else(|| anyhow::anyhow!("the daemon closed the connection without replying"))?;
    Ok(serde_json::from_str(&reply)?)
}

/// Runs `request` off the main thread and delivers the result back onto it.
///
/// `on_done` runs on the GTK main loop, so it may touch widgets freely.
pub fn send<F>(request: Request, on_done: F)
where
    F: FnOnce(anyhow::Result<Response>) + 'static,
{
    let (tx, rx) = async_channel::bounded(1);
    runtime().spawn(async move {
        let _ = tx.send(exchange(request).await).await;
    });
    gtk::glib::spawn_future_local(async move {
        if let Ok(result) = rx.recv().await {
            on_done(result);
        }
    });
}

/// Opens a pairing window and streams its events until it closes.
///
/// A separate function from [`send`] because `Pair` is the one request that
/// keeps its connection open: the QR payload arrives first, then a
/// confirmation request when a device proves it holds the code, and the
/// answer has to travel back down the *same* connection. The writer half is
/// therefore kept alive and handed to the caller as `confirm`.
pub fn pair<F>(ttl_secs: Option<u64>, mut on_event: F) -> PairHandle
where
    F: FnMut(anyhow::Result<Event>) -> bool + 'static,
{
    let (event_tx, event_rx) = async_channel::bounded::<anyhow::Result<Event>>(8);
    let (confirm_tx, confirm_rx) = async_channel::bounded::<bool>(1);

    runtime().spawn(async move {
        let path = control_socket_path();
        let stream = match UnixStream::connect(&path).await {
            Ok(s) => s,
            Err(e) => {
                let _ = event_tx
                    .send(Err(anyhow::anyhow!(
                        "could not reach the AnyFlow daemon at {}: {e}",
                        path.display()
                    )))
                    .await;
                return;
            }
        };
        let (read, mut write) = stream.into_split();
        let mut line = match serde_json::to_vec(&Request::Pair { ttl_secs }) {
            Ok(v) => v,
            Err(e) => {
                let _ = event_tx.send(Err(e.into())).await;
                return;
            }
        };
        line.push(b'\n');
        if write.write_all(&line).await.is_err() {
            let _ = event_tx
                .send(Err(anyhow::anyhow!("could not start pairing")))
                .await;
            return;
        }
        let _ = write.flush().await;

        // The confirmation travels back on this same connection, so a second
        // task owns the writer while the first reads events.
        tokio::spawn(async move {
            if let Ok(accept) = confirm_rx.recv().await {
                if let Ok(mut bytes) = serde_json::to_vec(&Request::Confirm { accept }) {
                    bytes.push(b'\n');
                    let _ = write.write_all(&bytes).await;
                    let _ = write.flush().await;
                }
            }
        });

        let mut lines = BufReader::new(read).lines();
        while let Ok(Some(text)) = lines.next_line().await {
            // The daemon may answer with a plain error instead of an event.
            if let Ok(Response::Error { message }) = serde_json::from_str::<Response>(&text) {
                let _ = event_tx.send(Err(anyhow::anyhow!(message))).await;
                break;
            }
            match serde_json::from_str::<Event>(&text) {
                Ok(event) => {
                    if event_tx.send(Ok(event)).await.is_err() {
                        break;
                    }
                }
                Err(e) => {
                    let _ = event_tx.send(Err(e.into())).await;
                    break;
                }
            }
        }
    });

    let rx = event_rx.clone();
    gtk::glib::spawn_future_local(async move {
        while let Ok(event) = rx.recv().await {
            // The callback returns false when the stream should stop being
            // watched — the window closed, or a terminal event arrived.
            if !on_event(event) {
                break;
            }
        }
    });

    PairHandle {
        confirm: confirm_tx,
        events: event_rx,
    }
}

/// Keeps a pairing stream alive and carries the human's answer back to it.
pub struct PairHandle {
    confirm: async_channel::Sender<bool>,
    events: async_channel::Receiver<anyhow::Result<Event>>,
}

impl PairHandle {
    /// Answers the daemon's `ConfirmRequest`.
    pub fn confirm(&self, accept: bool) {
        let tx = self.confirm.clone();
        runtime().spawn(async move {
            let _ = tx.send(accept).await;
        });
    }

    /// Drops the pairing window: closing the receiver ends the reader task.
    pub fn cancel(&self) {
        self.events.close();
        self.confirm.close();
    }
}

impl Drop for PairHandle {
    fn drop(&mut self) {
        self.cancel();
    }
}
