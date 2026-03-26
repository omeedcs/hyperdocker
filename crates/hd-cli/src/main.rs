use clap::{Parser, Subcommand};

mod commands;

#[derive(Parser)]
#[command(name = "hd", about = "Hyperdocker — incremental container runtime")]
#[command(version)]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    /// Create a new hd.toml in the current directory
    Init,
    /// Start environment from hd.toml
    Up,
    /// Stop the environment
    Down,
    /// Show environment status
    Status,
    /// Run a command in the sandbox
    Exec {
        /// Command to run
        #[arg(trailing_var_arg = true)]
        cmd: Vec<String>,
    },
    /// Translate a Dockerfile into hd.toml
    Ingest {
        /// Path to Dockerfile
        path: String,
    },
    /// Generate or update hd.lock
    Lock,
    /// Inspect the DAG
    Dag {
        #[command(subcommand)]
        action: DagAction,
    },
    /// CAS management
    Cas {
        #[command(subcommand)]
        action: CasAction,
    },
}

#[derive(Subcommand)]
enum DagAction {
    /// Show the current DAG tree
    Show,
}

#[derive(Subcommand)]
enum CasAction {
    /// Show CAS storage stats
    Stats,
    /// Run garbage collection
    Gc,
}

fn main() {
    let cli = Cli::parse();

    let result = match cli.command {
        Commands::Init => commands::init::run(),
        Commands::Up => commands::up::run(),
        Commands::Down => commands::down::run(),
        Commands::Status => commands::status::run(),
        Commands::Exec { cmd } => commands::exec::run(&cmd),
        Commands::Ingest { path } => commands::ingest::run(&path),
        Commands::Lock => commands::lock::run(),
        Commands::Dag { action } => match action {
            DagAction::Show => commands::dag::run_show(),
        },
        Commands::Cas { action } => match action {
            CasAction::Stats => commands::cas::run_stats(),
            CasAction::Gc => commands::cas::run_gc(),
        },
    };

    if let Err(e) = result {
        eprintln!("error: {}", e);
        std::process::exit(1);
    }
}
