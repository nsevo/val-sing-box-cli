use std::time::Duration;

use console::{Color, Style, Term, style};
use dialoguer::theme::ColorfulTheme;
use indicatif::{ProgressBar, ProgressStyle};

const BG_CARD: Color = Color::Color256(236);
const BG_HEADER: Color = Color::Color256(238);
const DIM: Color = Color::Color256(245);
const SPINNER_FRAMES: &[&str] = &["⠋", "⠙", "⠹", "⠸", "⠼", "⠴", "⠦", "⠧", "⠇", "⠏"];

fn bracketed_tag(tag: &str, s: &Style) -> String {
    let inner = format!("[{tag}]");
    let padded = format!("{inner:<6}");
    format!("  {}   ", s.apply_to(padded))
}

// ── Tag output ────────────────────────────────────────────────────────

pub fn print_ok(msg: &str) {
    let prefix = bracketed_tag("ok", &Style::new().green().bold());
    println!("{prefix}{msg}");
}

pub fn print_fail(msg: &str) {
    let prefix = bracketed_tag("fail", &Style::new().red().bold());
    eprintln!("{prefix}{msg}");
}

pub fn print_warn(msg: &str) {
    let prefix = bracketed_tag("warn", &Style::new().yellow().bold());
    eprintln!("{prefix}{msg}");
}

pub fn print_info(msg: &str) {
    let prefix = bracketed_tag("info", &Style::new().cyan());
    eprintln!("{prefix}{msg}");
}

pub fn print_hint(msg: &str) {
    eprintln!("         {}", style(msg).dim());
}

// ── Section header (with background) ──────────────────────────────────

pub fn print_header(title: &str) {
    let padded = format!("  {title}  ");
    println!(
        "  {}",
        Style::new().bg(BG_HEADER).white().bold().apply_to(padded)
    );
}

// ── Key-value lines ──────────────────────────────────────────────────

pub fn print_kv(label: &str, value: &str) {
    println!("  {:<11}{}", Style::new().fg(DIM).apply_to(label), value);
}

pub fn print_kv_highlight(label: &str, value: &str) {
    println!(
        "  {:<11}{}",
        Style::new().fg(DIM).apply_to(label),
        bg_highlight(value)
    );
}

pub fn print_kv_colored(label: &str, value: &str, color: Color) {
    println!(
        "  {:<11}{}",
        Style::new().fg(DIM).apply_to(label),
        Style::new().fg(color).apply_to(value)
    );
}

// ── Status badge ─────────────────────────────────────────────────────

pub fn print_status_running() {
    let tag = Style::new().green().bold().apply_to(" running ");
    println!(
        "  {:<11}{}",
        Style::new().fg(DIM).apply_to("State"),
        Style::new().bg(Color::Color256(22)).apply_to(tag)
    );
}

pub fn print_status_stopped() {
    let tag = Style::new().fg(DIM).apply_to(" stopped ");
    println!(
        "  {:<11}{}",
        Style::new().fg(DIM).apply_to("State"),
        Style::new().bg(BG_CARD).apply_to(tag)
    );
}

// ── Detail block (indented, for start/restart post-info) ─────────────

pub fn print_detail(label: &str, value: &str) {
    println!(
        "         {:<10} {}",
        style(label).bold(),
        bg_highlight(value)
    );
}

// ── Inline background highlight ──────────────────────────────────────

pub fn bg_highlight(value: &str) -> String {
    Style::new()
        .bg(BG_CARD)
        .apply_to(format!(" {value} "))
        .to_string()
}

// ── Latency coloring ─────────────────────────────────────────────────

pub fn format_latency(ms: u32) -> String {
    let label = format!("{ms}ms");
    if ms < 150 {
        style(label).green().to_string()
    } else if ms < 300 {
        style(label).yellow().to_string()
    } else {
        style(label).red().to_string()
    }
}

// ── Interactive select theme ─────────────────────────────────────────

pub fn select_theme() -> ColorfulTheme {
    ColorfulTheme {
        active_item_style: Style::new().bg(BG_CARD).cyan().bold(),
        active_item_prefix: style("▸ ".to_string()).cyan(),
        inactive_item_prefix: style("  ".to_string()).dim(),
        prompt_prefix: style("?".to_string()).cyan().bold(),
        fuzzy_match_highlight_style: Style::new().cyan().bold(),
        ..ColorfulTheme::default()
    }
}

// ── Spinner ──────────────────────────────────────────────────────────

pub fn spinner(msg: &str) -> ProgressBar {
    let is_tty = Term::stderr().is_term();

    let pb = ProgressBar::new_spinner();

    if is_tty {
        pb.set_style(
            ProgressStyle::default_spinner()
                .tick_chars("⠋⠙⠹⠸⠼⠴⠦⠧⠇⠏ ")
                .template("  {spinner:.cyan}      {msg}")
                .expect("valid spinner template"),
        );
        pb.enable_steady_tick(Duration::from_millis(80));
    } else {
        pb.set_style(
            ProgressStyle::default_spinner()
                .template("  ..     {msg}")
                .expect("valid spinner template"),
        );
    }

    pb.set_message(msg.to_string());
    pb
}

pub fn spinner_frame(step: usize) -> &'static str {
    SPINNER_FRAMES[step % SPINNER_FRAMES.len()]
}

pub fn finish_ok(pb: &ProgressBar, msg: &str) {
    let prefix = bracketed_tag("ok", &Style::new().green().bold());
    set_finish_style(pb);
    pb.finish_with_message(format!("{prefix}{msg}"));
}

pub fn finish_fail(pb: &ProgressBar, msg: &str) {
    let prefix = bracketed_tag("fail", &Style::new().red().bold());
    set_finish_style(pb);
    pb.finish_with_message(format!("{prefix}{msg}"));
}

fn set_finish_style(pb: &ProgressBar) {
    pb.set_style(
        ProgressStyle::default_spinner()
            .template("{msg}")
            .expect("valid template"),
    );
}
