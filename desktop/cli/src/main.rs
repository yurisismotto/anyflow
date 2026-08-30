//! `anyflow` — control the local daemon.
//!
//! Talks to the daemon over its Unix control socket. It holds no keys, no
//! trust store and no protocol logic: if the daemon is not running, every
//! command fails cleanly rather than doing something partial.

use std::time::Duration;

use anyflow_daemon::control::{
    control_socket_path, BatteryReport, DeviceReport, Event, Request, Response,
};
use clap::{Parser, Subcommand};
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::net::UnixStream;

#[derive(Parser, Debug)]
#[command(name = "anyflow", about = "AnyFlow control", version)]
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
             Start it with: systemctl --user start anyflowd.service",
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
            println!("AnyFlow");
            println!("  device      {} ({})", s.device_name, s.device_id);
            println!("  fingerprint {}", s.fingerprint_short);
            println!(
                "  listening   port {} ({})",
                s.listen_port, s.listen_families
            );
            println!(
                "  protocol    v{}..v{}",
                s.protocol_version_min, s.protocol_version_max
            );
            println!("  capabilities {}", s.capabilities.join(", "));
            println!("  paired      {} device(s)", s.paired_devices);
            if s.pairing_active {
                println!("  pairing     window OPEN");
            }

            if s.devices.is_empty() {
                println!("\n  no paired devices. Run: anyflow pair");
            } else {
                println!("\n  devices:");
                for d in &s.devices {
                    print_device(d, "    ");
                }
            }
        }
        Response::Devices(devices) => {
            if devices.is_empty() {
                println!("no paired devices. Run: anyflow pair");
                return Ok(());
            }
            for d in &devices {
                print_device(d, "   ");
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

/// Prints one device, keeping "paired", "connected" and "last known battery"
/// visibly separate.
///
/// They are three different facts and conflating them is how a dead session
/// ends up displayed as a live one. `paired` comes from the trust store,
/// `state` from whether a session exists right now, and a battery reading is
/// shown only with its age and only while a session is live.
fn print_device(d: &DeviceReport, indent: &str) {
    println!("{indent}{}  {}", d.device_name, d.device_id);
    println!("{indent}   platform    {}", d.platform);
    println!("{indent}   fingerprint {}", d.fingerprint_short);
    println!("{indent}   paired      {}", yes_no(d.paired));
    println!("{indent}   connected   {}", yes_no(d.connected));
    println!("{indent}   state       {}", d.state);
    if let Some(secs) = d.silent_secs {
        println!("{indent}   last frame  {} ago", human_duration(secs));
    }
    if let Some(secs) = d.last_seen_secs_ago {
        if !d.connected {
            println!("{indent}   last seen   {} ago", human_duration(secs));
        }
    }
    println!(
        "{indent}   granted     {}",
        if d.granted_capabilities.is_empty() {
            "-".to_string()
        } else {
            d.granted_capabilities.join(", ")
        }
    );
    if let Some(b) = &d.battery {
        println!("{indent}   battery     {}", battery_line(b));
    }
}

fn battery_line(b: &BatteryReport) -> String {
    format!(
        "{}% ({}, {} old{})",
        b.percentage,
        b.charging_state,
        human_duration(b.age_secs),
        if b.stale { "; STALE" } else { "" },
    )
}

fn yes_no(v: bool) -> &'static str {
    if v {
        "yes"
    } else {
        "no"
    }
}

fn human_duration(secs: u64) -> String {
    match secs {
        s if s < 60 => format!("{s}s"),
        s if s < 3600 => format!("{}m{:02}s", s / 60, s % 60),
        s => format!("{}h{:02}m", s / 3600, (s % 3600) / 60),
    }
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
                println!("Scan this with AnyFlow on your phone.");
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
                    "expired" => println!("\nPairing window expired. Run `anyflow pair` again."),
                    other => println!("\nPairing ended: {other} ({detail})"),
                }
                return Ok(());
            }
        }
    }
    Ok(())
}

async fn read_yes_no() -> bool {
    use std::io::Write as _;
    use tokio::io::AsyncBufReadExt;

    // The prompt is written with `print!`, so it sits in `std::io::stdout`'s
    // line buffer with no newline to push it out. Flushing tokio's stdout
    // here would flush a different handle and leave the question invisible:
    // the operator would see the fingerprint, no prompt, and a silent
    // decline 60 seconds later. Flush the handle the prompt was written to.
    let _ = std::io::stdout().flush();

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
