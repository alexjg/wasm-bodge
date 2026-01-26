use anyhow::Result;
use clap::{Parser, Subcommand};
use std::path::PathBuf;

mod build;
mod config;

#[derive(Parser)]
#[command(name = "wasm-bodge")]
#[command(about = "A tool that takes wasm-bindgen output and wraps it for all JavaScript runtimes")]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    /// Build an npm package from a wasm-bindgen Rust crate
    Build {
        /// Path to the Rust crate directory [default: current directory]
        #[arg(long, default_value = ".")]
        crate_path: PathBuf,

        /// Path to template package.json
        #[arg(long, default_value = "./package.json")]
        package_json: PathBuf,

        /// Output directory
        #[arg(long, default_value = "./dist")]
        out_dir: PathBuf,

        /// Cargo build profile
        #[arg(long, default_value = "release")]
        profile: String,

        /// Use prebuilt wasm-bindgen output from tarball
        #[arg(long)]
        wasm_bindgen_tar: Option<PathBuf>,
    },
}

fn main() -> Result<()> {
    let cli = Cli::parse();

    match cli.command {
        Commands::Build {
            crate_path,
            package_json,
            out_dir,
            profile,
            wasm_bindgen_tar,
        } => {
            let config = config::BuildConfig {
                crate_path,
                package_json,
                out_dir,
                profile,
                wasm_bindgen_tar,
            };
            build::run(config)?;
        }
    }

    Ok(())
}
