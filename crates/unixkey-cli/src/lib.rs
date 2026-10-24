use std::io::{BufRead, BufReader, Write};
use std::os::unix::net::UnixStream;
use std::thread;
use std::time::Duration;

use anyhow::{Context, Result, bail};
use clap::{Parser, Subcommand, ValueEnum};
use serde::Serialize;
use unixkey_core::{
    CommitMode, DaemonStatus, InputMethod, IpcRequest, IpcResponse, ToneStyle, runtime_socket_path,
};

#[derive(Parser, Debug)]
#[command(
    name = "unixkey-ctl",
    version,
    about = "Control CLI & Waybar status provider for UniXKey (Vietnamese IME for Niri)"
)]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand, Debug)]
enum Commands {
    /// Query current IME status (supports --watch --json for Waybar)
    Status {
        /// Output Waybar-compatible JSON
        #[arg(long)]
        json: bool,
        /// Continuously stream status updates as newline-delimited JSON
        #[arg(long)]
        watch: bool,
    },
    /// Toggle between Vietnamese (VN) and English (EN)
    Toggle,
    /// Explicitly set language state (vn or en)
    Set {
        #[arg(value_enum)]
        state: LangStateArg,
    },
    /// Set input method (telex or vni)
    Method {
        #[arg(value_enum)]
        method: MethodArg,
    },
    /// Set tone placement style (modern: hoà, traditional: hòa)
    Tone {
        #[arg(value_enum)]
        style: ToneArg,
    },
    /// Set text commit mode (backspace, surrounding, preedit)
    CommitMode {
        #[arg(value_enum)]
        mode: CommitModeArg,
    },
    /// List open Niri windows tracked by unixkey-daemon
    Windows {
        #[arg(long)]
        json: bool,
    },
    /// Reload configuration from ~/.config/unixkey/config.toml
    Reload,
    /// Stop unixkey-daemon
    Quit,
}

#[derive(ValueEnum, Clone, Copy, Debug)]
enum LangStateArg {
    Vn,
    En,
}

#[derive(ValueEnum, Clone, Copy, Debug)]
enum MethodArg {
    Telex,
    Vni,
}

#[derive(ValueEnum, Clone, Copy, Debug)]
enum ToneArg {
    Modern,
    Traditional,
}

#[derive(ValueEnum, Clone, Copy, Debug)]
enum CommitModeArg {
    Preedit,
}

#[derive(Serialize)]
struct WaybarOutput {
    text: &'static str,
    alt: &'static str,
    tooltip: String,
    class: &'static str,
}

impl WaybarOutput {
    fn from_status(st: &DaemonStatus) -> Self {
        let (text, alt, class) = if st.enabled {
            ("VN", "vn", "vn")
        } else {
            ("EN", "en", "en")
        };

        let method = match st.input_method {
            InputMethod::Telex => "Telex",
            InputMethod::Vni => "VNI",
        };
        let tone = match st.tone_style {
            ToneStyle::Modern => "Modern (hoà)",
            ToneStyle::Traditional => "Traditional (hòa)",
        };
        let win_str = st
            .focused_window
            .as_ref()
            .and_then(|w| w.app_id.as_deref())
            .unwrap_or("none");

        let tooltip = format!(
            "ViKey: {} ({})\nTone: {}\nCommit: {}\nFocused App: {}",
            if st.enabled {
                "Tiếng Việt"
            } else {
                "English"
            },
            method,
            tone,
            st.active_commit_mode.label(),
            win_str
        );

        Self {
            text,
            alt,
            tooltip,
            class,
        }
    }
}

fn send_request(req: &IpcRequest) -> Result<IpcResponse> {
    let sock_path = runtime_socket_path();
    let mut stream = UnixStream::connect(&sock_path).with_context(|| {
        format!(
            "Could not connect to unixkey-daemon at {}. Is unixkey-daemon running?",
            sock_path.display()
        )
    })?;

    let mut payload = serde_json::to_string(req)?;
    payload.push('\n');
    stream.write_all(payload.as_bytes())?;
    stream.flush()?;

    let mut reader = BufReader::new(stream);
    let mut line = String::new();
    reader.read_line(&mut line)?;

    let resp: IpcResponse =
        serde_json::from_str(line.trim()).context("Invalid response from unixkey-daemon")?;
    Ok(resp)
}

fn print_status(st: &DaemonStatus, json: bool) -> Result<()> {
    if json {
        let wb = WaybarOutput::from_status(st);
        println!("{}", serde_json::to_string(&wb)?);
    } else {
        println!(
            "State:        {}",
            if st.enabled { "VN (Tiếng Việt)" } else { "EN (English)" }
        );
        println!("Input Method: {:?}", st.input_method);
        println!("Tone Style:   {:?}", st.tone_style);
        println!("Commit Mode:  {}", st.active_commit_mode.label());
        println!("IME Grab:     {}", if st.ime_grab_active { "Active" } else { "Idle" });
        if let Some(w) = &st.focused_window {
            println!(
                "Focused App:  {} (id={}, title={:?})",
                w.app_id.as_deref().unwrap_or("<unknown>"),
                w.id,
                w.title.as_deref().unwrap_or("")
            );
        }
    }
    Ok(())
}

fn watch_status(json: bool) -> Result<()> {
    let sock_path = runtime_socket_path();
    loop {
        match UnixStream::connect(&sock_path) {
            Ok(mut stream) => {
                let mut payload = serde_json::to_string(&IpcRequest::Subscribe)?;
                payload.push('\n');
                if stream.write_all(payload.as_bytes()).is_ok() {
                    let reader = BufReader::new(stream);
                    for line in reader.lines() {
                        let Ok(line) = line else { break };
                        if line.trim().is_empty() {
                            continue;
                        }
                        if let Ok(IpcResponse::Status(st)) = serde_json::from_str::<IpcResponse>(&line) {
                            let _ = print_status(&st, json);
                        }
                    }
                }
            }
            Err(_) => {
                if json {
                    let offline = WaybarOutput {
                        text: "--",
                        alt: "offline",
                        tooltip: "unixkey-daemon is not running".to_string(),
                        class: "offline",
                    };
                    println!("{}", serde_json::to_string(&offline)?);
                }
            }
        }
        thread::sleep(Duration::from_secs(2));
    }
}

pub fn run() -> Result<()> {
    let cli = Cli::parse();

    match cli.command {
        Commands::Status { json, watch } => {
            if watch {
                watch_status(json)?;
            } else {
                match send_request(&IpcRequest::GetStatus)? {
                    IpcResponse::Status(st) => print_status(&st, json)?,
                    IpcResponse::Error(e) => bail!("Daemon error: {e}"),
                    other => bail!("Unexpected response: {other:?}"),
                }
            }
        }
        Commands::Toggle => match send_request(&IpcRequest::Toggle)? {
            IpcResponse::Status(st) => print_status(&st, false)?,
            IpcResponse::Ok => {}
            IpcResponse::Error(e) => bail!("Daemon error: {e}"),
            other => bail!("Unexpected response: {other:?}"),
        },
        Commands::Set { state } => {
            let en = matches!(state, LangStateArg::Vn);
            match send_request(&IpcRequest::SetEnabled(en))? {
                IpcResponse::Status(st) => print_status(&st, false)?,
                IpcResponse::Ok => {}
                IpcResponse::Error(e) => bail!("Daemon error: {e}"),
                other => bail!("Unexpected response: {other:?}"),
            }
        }
        Commands::Method { method } => {
            let m = match method {
                MethodArg::Telex => InputMethod::Telex,
                MethodArg::Vni => InputMethod::Vni,
            };
            match send_request(&IpcRequest::SetInputMethod(m))? {
                IpcResponse::Status(st) => print_status(&st, false)?,
                IpcResponse::Ok => {}
                IpcResponse::Error(e) => bail!("Daemon error: {e}"),
                other => bail!("Unexpected response: {other:?}"),
            }
        }
        Commands::Tone { style } => {
            let s = match style {
                ToneArg::Modern => ToneStyle::Modern,
                ToneArg::Traditional => ToneStyle::Traditional,
            };
            match send_request(&IpcRequest::SetToneStyle(s))? {
                IpcResponse::Status(st) => print_status(&st, false)?,
                IpcResponse::Ok => {}
                IpcResponse::Error(e) => bail!("Daemon error: {e}"),
                other => bail!("Unexpected response: {other:?}"),
            }
        }
        Commands::CommitMode { mode: _ } => {
            match send_request(&IpcRequest::SetCommitMode(CommitMode::Preedit))? {
                IpcResponse::Status(st) => print_status(&st, false)?,
                IpcResponse::Ok => {}
                IpcResponse::Error(e) => bail!("Daemon error: {e}"),
                other => bail!("Unexpected response: {other:?}"),
            }
        }
        Commands::Windows { json } => match send_request(&IpcRequest::ListNiriWindows)? {
            IpcResponse::Windows(wins) => {
                if json {
                    println!("{}", serde_json::to_string_pretty(&wins)?);
                } else if wins.is_empty() {
                    println!("No Niri windows reported (is NIRI_SOCKET set?).");
                } else {
                    for w in wins {
                        let focus_mark = if w.is_focused { "*" } else { " " };
                        println!(
                            "{} [{}] app_id={:?} title={:?}",
                            focus_mark,
                            w.id,
                            w.app_id.as_deref().unwrap_or(""),
                            w.title.as_deref().unwrap_or("")
                        );
                    }
                }
            }
            IpcResponse::Error(e) => bail!("Daemon error: {e}"),
            other => bail!("Unexpected response: {other:?}"),
        },
        Commands::Reload => match send_request(&IpcRequest::ReloadConfig)? {
            IpcResponse::Ok | IpcResponse::Status(_) => println!("Configuration reloaded."),
            IpcResponse::Error(e) => bail!("Daemon error: {e}"),
            other => bail!("Unexpected response: {other:?}"),
        },
        Commands::Quit => match send_request(&IpcRequest::Quit)? {
            IpcResponse::Ok => println!("Stopping unixkey-daemon."),
            IpcResponse::Error(e) => bail!("Daemon error: {e}"),
            other => bail!("Unexpected response: {other:?}"),
        },
    }

    Ok(())
}
