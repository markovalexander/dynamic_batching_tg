use anyhow::Result;
use clap::Parser;

use nix::sys::signal::{self, Signal};
use nix::unistd::Pid;
use std::process::{Child, Command, ExitStatus};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::thread::sleep;
use std::time::{Duration, Instant};

#[derive(Debug)]
enum LauncherError {
    TGBotError,
    ServerError,
    RouterError,
}

#[derive(Debug)]
enum ProgramName {
    TGBot,
    GrpcServer,
    Router,
}

impl ProgramName {
    fn from_str(program_name: &str) -> Option<Self> {
        match program_name {
            "telegram_bot" => Some(ProgramName::TGBot),
            "grpc_server" => Some(ProgramName::GrpcServer),
            "router" => Some(ProgramName::Router),
            _ => None,
        }
    }

    fn get_error(&self) -> LauncherError {
        match self {
            ProgramName::TGBot => LauncherError::TGBotError,
            ProgramName::GrpcServer => LauncherError::ServerError,
            ProgramName::Router => LauncherError::RouterError,
        }
    }

    fn get_executable(&self, debug: bool) -> String {
        let executable_name = match self {
            ProgramName::TGBot => "telegram_bot",
            ProgramName::GrpcServer => "server",
            ProgramName::Router => "router",
        };

        if debug {
            tracing::warn!("Running in debug mode, remove before pushing");
            let prefix = "/Users/amarkov/Documents/Projects/rust/batching_service/target/debug/";
            format!("{}{}", prefix, executable_name)
        } else {
            executable_name.to_string()
        }
    }
}

#[derive(Parser, Debug)]
struct Args {
    reply_server_address: String,
    reply_server_port: u16,
    grpc_port: u16,
    #[clap(long, short, env = "TG_TOKEN")]
    tg_token: String,
    #[clap(long, short, default_value = "false")]
    debug: bool,
}

impl Args {
    fn get_arguments(&self, program_name: &ProgramName) -> Vec<String> {
        match program_name {
            ProgramName::GrpcServer => vec!["--port".to_string(), self.grpc_port.to_string()],
            ProgramName::Router => vec![
                "--address".to_string(),
                format!("{}:{}", self.reply_server_address, self.reply_server_port),
                "--grpc-address".to_string(),
                format!("127.0.0.1:{}", self.grpc_port),
            ],
            ProgramName::TGBot => vec![
                "--reply-server-address".to_string(),
                format!("{}:{}", self.reply_server_address, self.reply_server_port),
                "--tg-token".to_string(),
                self.tg_token.clone(),
            ],
        }
    }
}

fn main() -> Result<(), LauncherError> {
    tracing_subscriber::fmt()
        .with_max_level(tracing::Level::INFO)
        .init();

    let args = Args::parse();
    tracing::info!("Launcher started with {:?}", args);

    let running = Arc::new(AtomicBool::new(true));
    let r = running.clone();

    ctrlc::set_handler(move || {
        r.store(false, Ordering::SeqCst);
    })
    .expect("Error setting Ctrl-C handler");

    let grpc_server = spawn_program(&args, "grpc_server").expect("Failed to start grpc_server");

    sleep(Duration::from_millis(100));
    let router = spawn_program(&args, "router").expect("Failed to start router");

    let mut tg_bot = spawn_program(&args, "telegram_bot").expect("Failed to start telegram_bot");

    tracing::info!("Everything is up and running. Press Ctrl-C to terminate the application.");

    while running.load(Ordering::SeqCst) {
        match tg_bot.try_wait().unwrap() {
            Some(_) => {
                tracing::error!("TG bot has died, shutting down");
                return Err(LauncherError::ServerError);
            }
            None => sleep(Duration::from_millis(100)),
        }
    }

    terminate("grpc_server", grpc_server, Duration::from_millis(100))
        .expect("Failed to terminate grpc_server");
    terminate("router", router, Duration::from_millis(100)).expect("Failed to terminate router");
    terminate("telegram_bot", tg_bot, Duration::from_millis(100))
        .expect("Failed to terminate telegram_bot");

    Ok(())
}

fn spawn_program(args: &Args, program_name: &str) -> Result<Child, LauncherError> {
    tracing::info!("Spawning {}", program_name);

    let program_type = ProgramName::from_str(program_name).unwrap();
    let program_args = args.get_arguments(&program_type);

    let child = Command::new(program_type.get_executable(args.debug))
        .args(program_args)
        .spawn()
        .map_err(|_| program_type.get_error())?;

    Ok(child)
}

fn terminate(process_name: &str, mut process: Child, timeout: Duration) -> Result<ExitStatus> {
    tracing::info!("Terminating {process_name}");

    let terminate_time = Instant::now();
    signal::kill(Pid::from_raw(process.id() as i32), Signal::SIGTERM).unwrap();

    tracing::info!("Waiting for {process_name} to gracefully shutdown");

    while terminate_time.elapsed() < timeout {
        if let Some(status) = process.try_wait()? {
            tracing::info!("{process_name} terminated");
            return Ok(status);
        }
        sleep(Duration::from_millis(100));
    }

    tracing::info!("Killing {process_name}");

    process.kill()?;
    let exit_status = process.wait()?;

    tracing::info!("{process_name} killed");
    Ok(exit_status)
}
