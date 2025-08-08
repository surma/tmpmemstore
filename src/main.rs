use anyhow::bail;
use anyhow::{Context, Result};
use clap::{Parser, Subcommand};
use rpassword::prompt_password;
use std::env;
use std::fs;
use std::io::Write;
use std::os::unix::fs::PermissionsExt;
use std::os::unix::net::{UnixListener, UnixStream};
use std::path::{Path, PathBuf};
use std::process::Command;
use std::process::Stdio;
use std::thread;

mod process_tree;

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
        /// Read data from file instead of prompting (use '-' for stdin)
        #[arg(short = 'i', long = "input", value_name = "FILE")]
        input: Option<String>,
        /// Path to create the UNIX socket
        #[arg(short = 's', long = "socket", value_name = "PATH")]
        socket: Option<String>,
        /// Command and arguments to run
        #[arg(last = true, required = true)]
        command: Vec<String>,
    },
    /// Retrieve stored data
    Retrieve {
        /// Path to the UNIX socket
        #[arg(short = 's', long = "socket", value_name = "PATH")]
        socket: Option<String>,
    },
}

fn main() -> Result<()> {
    let cli = Cli::parse();

    match cli.command {
        Commands::Run {
            input,
            socket,
            command,
        } => run_command(input, socket, command),
        Commands::Retrieve { socket } => retrieve_data(socket),
    }
}

fn read_input_data(input: Option<String>) -> Result<String> {
    let Some(file_path) = input else {
        return prompt_password("Enter data to store: ").context("Failed to read password");
    };
    if file_path == "-" {
        std::fs::read_to_string(file_path).context("Failed to read from stdin")
    } else {
        std::fs::read_to_string(&file_path)
            .with_context(|| format!("Failed to read file: {}", file_path))
    }
}

struct TempFile {
    _dir: Option<tempfile::TempDir>,
    path: PathBuf,
}

impl TempFile {
    fn in_tempfolder_with_name(name: impl AsRef<Path>) -> Result<Self> {
        let dir = tempfile::Builder::new()
            .prefix("tmpmemstore-")
            .tempdir()
            .context("Failed to create temporary directory")?;
        let path = dir.path().join(name);
        Ok(TempFile {
            _dir: Some(dir),
            path,
        })
    }
}

impl From<PathBuf> for TempFile {
    fn from(path: PathBuf) -> Self {
        TempFile { _dir: None, path }
    }
}

impl Drop for TempFile {
    fn drop(&mut self) {
        if let Err(e) = std::fs::remove_file(self) {
            eprintln!("Error deleting socket: {e}");
        }
    }
}

impl AsRef<Path> for TempFile {
    fn as_ref(&self) -> &Path {
        &self.path
    }
}

fn run_command(input: Option<String>, socket: Option<String>, command: Vec<String>) -> Result<()> {
    let data = read_input_data(input)?;

    let socket_path = socket
        .map(|v| Ok(PathBuf::from(v).into()))
        .unwrap_or_else(|| TempFile::in_tempfolder_with_name("socket"))?;

    if let Some(parent) = socket_path.as_ref().parent() {
        fs::create_dir_all(parent)?
    }

    let listener = UnixListener::bind(socket_path.as_ref()).context("Failed to bind socket")?;
    std::fs::set_permissions(socket_path.as_ref(), std::fs::Permissions::from_mode(0o666))
        .context("Setting permissions")?;

    thread::spawn(move || {
        for stream in listener.incoming() {
            if let Err(e) = handle_stream(&data, stream) {
                eprintln!("Error handling connection: {e}");
            }
        }
    });

    let socket_path_str = socket_path
        .as_ref()
        .to_str()
        .ok_or_else(|| anyhow::anyhow!("Socket path contains invalid UTF-8"))?;

    let mut child = Command::new(&command[0])
        .args(&command[1..])
        .env("TMPMEMSTORE_SOCKET", socket_path_str)
        .current_dir(std::env::current_dir()?)
        .stdin(Stdio::inherit())
        .stdout(Stdio::inherit())
        .stderr(Stdio::inherit())
        .spawn()
        .context("Failed to spawn subprocess")?;

    let status = child.wait()?;

    // Keep socket available til after child has exited.
    drop(socket_path);
    std::process::exit(status.code().unwrap_or(1));
}

fn handle_stream(
    data: &String,
    stream: std::result::Result<UnixStream, std::io::Error>,
) -> Result<()> {
    let parent_pid = std::process::id();
    let mut stream = stream.context("Accepting connection")?;
    if !process_tree::client_is_descendant(&stream, parent_pid)
        .context("Verifying connecting process is subprocess")?
    {
        bail!("Client is not descendant");
    }
    stream.write_all(data.as_bytes()).context("Writing data")?;
    Ok(())
}

fn retrieve_data(socket: Option<String>) -> Result<()> {
    let socket_path = if let Some(socket_path) = socket {
        socket_path
    } else if let Ok(socket_path) = env::var("TMPMEMSTORE_SOCKET") {
        socket_path
    } else {
        bail!("No socket path provided via CLI flag or TMPMEMSTORE_SOCKET")
    };

    let mut stream = UnixStream::connect(&socket_path).context("Failed to connect to socket")?;

    std::io::copy(&mut stream, &mut std::io::stdout()).context("Failed to read from socket")?;
    Ok(())
}
