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

        /// Cargo build profile for the release variant (optimized; wasm-opt
        /// applied unless --no-wasm-opt is set).
        #[arg(long, alias = "profile", default_value = "release")]
        release_profile: String,

        /// Cargo build profile for the debug variant. Passing this flag also
        /// builds a parallel `/debug` subpath export. Profile name must
        /// match a `[profile.<name>]` section in the authoritative Cargo.toml.
        #[arg(long)]
        debug_profile: Option<String>,

        /// Use prebuilt wasm-bindgen output from tarball
        #[arg(long)]
        wasm_bindgen_tar: Option<PathBuf>,

        /// Disable wasm-opt optimization
        #[arg(long, default_value_t = false)]
        no_wasm_opt: bool,
    },
}

fn main() -> Result<()> {
    let cli = Cli::parse();

    match cli.command {
        Commands::Build {
            crate_path,
            package_json,
            out_dir,
            release_profile,
            debug_profile,
            wasm_bindgen_tar,
            no_wasm_opt,
        } => {
            let config = config::BuildConfig {
                crate_path,
                package_json,
                out_dir,
                release_profile,
                debug_profile,
                wasm_bindgen_tar,
                wasm_opt: !no_wasm_opt,
            };
            build::run(config)?;
        }
    }

    Ok(())
}
