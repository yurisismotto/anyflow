//! `fedroid` — control the local daemon.
//!
//! Talks to the daemon over its Unix control socket. It holds no keys, no
//! trust store and no protocol logic: if the daemon is not running, every
//! command fails cleanly rather than doing something partial.

use std::time::Duration;

use clap::{Parser, Subcommand};
use fedroid_daemon::control::{control_socket_path, Event, Request, Response};
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::net::UnixStream;

#[derive(Parser, Debug)]
#[command(name = "fedroid", about = "Fedroid Bridge control", version)]
struct Args {
    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand, Debug)]
enum Command {
    /// Daemon status, identity and live connections.
    Status,
    /// List paired devices.
    Devices,
    /// Open a pairing window and show a QR code.
    Pair {
        /// Seconds the window stays open.
        #[arg(long)]
        ttl: Option<u64>,
    },
    /// Revoke a pairing, by device id or fingerprint prefix.
    Unpair { device: String },
    /// Round-trip a PING over the live session with a device.
    Ping { device: String },
}

#[tokio::main(flavor = "current_thread")]
async fn main() -> anyhow::Result<()> {
    let args = Args::parse();

    let path = control_socket_path();
    let stream = UnixStream::connect(&path).await.map_err(|e| {
        anyhow::anyhow!(
            "cannot reach the daemon at {} ({e}).\n\
             Start it with: systemctl --user start fedroid-bridge.service",
            path.display()
        )
    })?;

    match args.command {
        Command::Status => simple(stream, Request::Status).await,
        Command::Devices => simple(stream, Request::Devices).await,
        Command::Ping { device } => simple(stream, Request::Ping { device }).await,
        Command::Unpair { device } => simple(stream, Request::Unpair { device }).await,
        Command::Pair { ttl } => pair(stream, ttl).await,
    }
}

async fn simple(stream: UnixStream, request: Request) -> anyhow::Result<()> {
    let (read, mut write) = stream.into_split();
    let mut lines = BufReader::new(read).lines();

    write_json(&mut write, &request).await?;
    let Some(line) = lines.next_line().await? else {
        anyhow::bail!("daemon closed the connection without replying");
    };

    match serde_json::from_str::<Response>(&line)? {
        Response::Status(s) => {
            println!("Fedroid Bridge");
            println!("  device      {} ({})", s.device_name, s.device_id);
            println!("  fingerprint {}", s.fingerprint_short);
            println!("  listening   port {}", s.listen_port);
            println!(
                "  protocol    v{}..v{}",
                s.protocol_version_min, s.protocol_version_max
            );
            println!("  capabilities {}", s.capabilities.join(", "));
            println!("  paired      {} device(s)", s.paired_devices);
            if s.pairing_active {
                println!("  pairing     window OPEN");
            }
            if s.connections.is_empty() {
                println!("\n  no active connections");
            } else {
                println!("\n  active connections:");
                for c in &s.connections {
                    println!(
                        "    {} [{}]  caps: {}",
                        c.device_name,
                        c.fingerprint_short,
                        if c.negotiated_capabilities.is_empty() {
                            "-".to_string()
                        } else {
                            c.negotiated_capabilities.join(", ")
                        }
                    );
                    if let Some(b) = &c.battery {
                        println!(
                            "      battery {}% ({}, {}s ago)",
                            b.percentage, b.charging_state, b.age_secs
                        );
                    }
                }
            }
        }
        Response::Devices(devices) => {
            if devices.is_empty() {
                println!("no paired devices. Run: fedroid pair");
                return Ok(());
            }
            for d in devices {
                let flag = if d.revoked {
                    "revoked"
                } else if d.connected {
                    "connected"
                } else {
                    "offline"
                };
                println!("{}  {} [{}]", d.device_name, d.device_id, flag);
                println!("   platform    {}", d.platform);
                println!("   fingerprint {}", d.fingerprint_short);
                println!(
                    "   granted     {}",
                    if d.granted_capabilities.is_empty() {
                        "-".to_string()
                    } else {
                        d.granted_capabilities.join(", ")
                    }
                );
            }
        }
        Response::Pong { rtt_ms } => println!("pong in {rtt_ms} ms"),
        Response::Ok { message } => println!("{message}"),
        Response::Error { message } => {
            eprintln!("error: {message}");
            std::process::exit(1);
        }
    }
    Ok(())
}

/// Runs an interactive pairing session.
///
/// The pairing window lives exactly as long as this process: closing the
/// terminal closes the window. That is a feature, not a limitation.
async fn pair(stream: UnixStream, ttl: Option<u64>) -> anyhow::Result<()> {
    let (read, mut write) = stream.into_split();
    let mut lines = BufReader::new(read).lines();

    write_json(&mut write, &Request::Pair { ttl_secs: ttl }).await?;

    while let Some(line) = lines.next_line().await? {
        // The daemon may send a plain Response (e.g. an error) instead of an
        // Event, so try both rather than failing on the first mismatch.
        if let Ok(Response::Error { message }) = serde_json::from_str::<Response>(&line) {
            eprintln!("error: {message}");
            std::process::exit(1);
        }

        match serde_json::from_str::<Event>(&line)? {
            Event::PairingReady {
                payload,
                qr_ascii,
                expires_in_secs,
            } => {
                println!("{qr_ascii}");
                println!("Scan this with Fedroid Bridge on your phone.");
                println!("Expires in {expires_in_secs}s. The code is single-use.\n");
                println!("If your phone cannot scan, the payload is:\n  {payload}\n");
            }

            Event::ConfirmRequest {
                device_name,
                device_id,
                fingerprint_short,
                ..
            } => {
                println!("A device proved it holds the pairing code:\n");
                println!("  name        {device_name}");
                println!("  device id   {device_id}");
                println!("  fingerprint {fingerprint_short}");
                println!("\nCheck that the fingerprint matches the one shown on the phone.");
                print!("Pair with this device? [y/N] ");

                let accept = read_yes_no().await;
                write_json(&mut write, &Request::Confirm { accept }).await?;
            }

            Event::Finished { status, detail } => {
                match status.as_str() {
                    "paired" => println!("\nPaired with {detail}."),
                    "declined" => println!("\nDeclined. {detail} was not paired."),
                    "expired" => println!("\nPairing window expired. Run `fedroid pair` again."),
                    other => println!("\nPairing ended: {other} ({detail})"),
                }
                return Ok(());
            }
        }
    }
    Ok(())
}

async fn read_yes_no() -> bool {
    use tokio::io::AsyncBufReadExt;
    let mut stdout = tokio::io::stdout();
    let _ = stdout.flush().await;

    let mut line = String::new();
    let mut reader = BufReader::new(tokio::io::stdin());
    // A read failure or EOF means nobody answered: the safe default is no.
    match tokio::time::timeout(Duration::from_secs(60), reader.read_line(&mut line)).await {
        Ok(Ok(_)) => matches!(line.trim().to_ascii_lowercase().as_str(), "y" | "yes"),
        _ => false,
    }
}

async fn write_json<W: AsyncWriteExt + Unpin>(w: &mut W, value: &Request) -> anyhow::Result<()> {
    let mut bytes = serde_json::to_vec(value)?;
    bytes.push(b'\n');
    w.write_all(&bytes).await?;
    w.flush().await?;
    Ok(())
}
