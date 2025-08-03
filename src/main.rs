use anyhow::{Context, Result};
use clap::{Parser, Subcommand};
use rpassword::prompt_password;
use std::env;
use std::fs;
use std::io::{Read, Write};
use std::os::unix::fs::PermissionsExt;
use std::os::unix::net::{UnixListener, UnixStream};
use std::process::Command;
use std::process::Stdio;
use std::sync::Arc;
use std::thread;

#[derive(Parser)]
#[command(name = "tmpmemstore")]
#[command(about = "Store data in memory and expose via UNIX socket", long_about = None)]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    /// Run a command with access to stored data
    Run {
        /// Command and arguments to run
        #[arg(last = true, required = true)]
        command: Vec<String>,
    },
    /// Retrieve stored data
    Retrieve,
}

fn main() -> Result<()> {
    let cli = Cli::parse();

    match cli.command {
        Commands::Run { command } => run_command(command),
        Commands::Retrieve => retrieve_data(),
    }
}

fn run_command(command: Vec<String>) -> Result<()> {
    let data = prompt_password("Enter data to store: ").context("Failed to read password")?;

    let socket_file = tempfile::Builder::new()
        .prefix("tmpmemstore-")
        .suffix(".sock")
        .tempfile()
        .context("Failed to create temporary file")?;
    let socket_path = socket_file.path();
    let listener = UnixListener::bind(dbg!(socket_path)).context("Failed to bind socket")?;
    let mut perms = fs::metadata(socket_path)?.permissions();
    perms.set_mode(0o600);
    fs::set_permissions(socket_path, perms).context("Failed to set socket permissions")?;

    let socket_path = socket_path
        .to_str()
        .ok_or_else(|| anyhow::anyhow!("Socket path contains invalid UTF-8"))?;

    thread::spawn(move || {
        for stream in listener.incoming() {
            match stream {
                Ok(mut stream) => {
                    if let Err(e) = stream.write_all(data.as_bytes()) {
                        eprintln!("Error writing to socket: {}", e);
                    }
                }
                Err(e) => {
                    eprintln!("Error accepting connection: {}", e);
                }
            }
        }
    });

    let mut child = Command::new(&command[0])
        .args(&command[1..])
        .env("TMPMEMSTORE_SOCKET", socket_path)
        .stdin(Stdio::inherit())
        .stdout(Stdio::inherit())
        .stderr(Stdio::inherit())
        .spawn()
        .context("Failed to spawn subprocess")?;

    let status = child.wait()?;

    std::process::exit(status.code().unwrap_or(1));
}

fn retrieve_data() -> Result<()> {
    let socket_path = env::var("TMPMEMSTORE_SOCKET")
        .context("TMPMEMSTORE_SOCKET environment variable not set")?;

    let mut stream = UnixStream::connect(&socket_path).context("Failed to connect to socket")?;

    let mut data = String::new();
    stream
        .read_to_string(&mut data)
        .context("Failed to read from socket")?;

    print!("{}", data);
    Ok(())
}
