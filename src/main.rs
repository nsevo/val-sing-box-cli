mod app;
mod clash;
mod cli;
mod config;
mod doctor;
mod errors;
mod install;
mod ip;
mod output;
mod platform;
mod service;
mod state;
mod subscription;
mod ui;
mod uninstall;

use clap::Parser;

use app::App;
use cli::{Cli, Commands};
use output::{JsonOutput, Renderer};

#[tokio::main]
async fn main() {
    let cli = Cli::parse();

    if let Commands::Completion { shell } = cli.command {
        cli::print_completion(shell);
        return;
    }

    let app = match App::new(cli.config_dir.as_deref(), cli.json, cli.verbose, cli.yes) {
        Ok(a) => a,
        Err(e) => {
            if cli.json {
                let renderer = Renderer::new(true);
                renderer.print_json(&JsonOutput::<()>::error(
                    "init",
                    e.error_code(),
                    e.to_string(),
                    e.hint().map(str::to_string),
                ));
            } else {
                ui::print_fail(&e.to_string());
                if let Some(hint) = e.hint() {
                    ui::print_hint(&format!("run: {hint}"));
                }
            }
            std::process::exit(e.exit_code());
        }
    };

    if let Err(e) = app.maybe_elevate(&cli.command) {
        if app.renderer.is_json() {
            app.renderer.print_json(&JsonOutput::<()>::error(
                "elevation",
                e.error_code(),
                e.to_string(),
                e.hint().map(str::to_string),
            ));
        } else {
            ui::print_fail(&e.to_string());
            if let Some(hint) = e.hint() {
                ui::print_hint(&format!("run: {hint}"));
            }
        }
        std::process::exit(e.exit_code());
    }

    if let Err(e) = app.run(cli.command).await {
        if app.renderer.is_json() {
            app.renderer.print_json(&JsonOutput::<()>::error(
                "error",
                e.error_code(),
                e.to_string(),
                e.hint().map(str::to_string),
            ));
        } else {
            ui::print_fail(&e.to_string());
            if let Some(hint) = e.hint() {
                ui::print_hint(&format!("run: {hint}"));
            }
        }
        std::process::exit(e.exit_code());
    }
}
