// fatcat - Hunt down the fat files hogging your disk space
// Copyright (C) 2024  rxxuzi
//
// This program is free software: you can redistribute it and/or modify
// it under the terms of the GNU General Public License as published by
// the Free Software Foundation, either version 3 of the License, or
// (at your option) any later version.

use chrono::Local;
use colored::*;
use indicatif::{ProgressBar, ProgressStyle};
use jwalk::WalkDir;
use std::env;
use std::fs::File;
use std::io::{BufWriter, Write};
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{Duration, Instant};

const VERSION: &str = env!("CARGO_PKG_VERSION");

#[derive(Debug, Clone)]
struct FileInfo {
    path: PathBuf,
    size: u64,
}

fn format_size(bytes: u64) -> String {
    const KB: u64 = 1024;
    const MB: u64 = KB * 1024;
    const GB: u64 = MB * 1024;
    const TB: u64 = GB * 1024;

    if bytes >= TB {
        format!("{:.2} TB", bytes as f64 / TB as f64)
    } else if bytes >= GB {
        format!("{:.2} GB", bytes as f64 / GB as f64)
    } else if bytes >= MB {
        format!("{:.2} MB", bytes as f64 / MB as f64)
    } else if bytes >= KB {
        format!("{:.2} KB", bytes as f64 / KB as f64)
    } else {
        format!("{} B", bytes)
    }
}

fn strip_ansi(s: &str) -> String {
    let mut result = String::with_capacity(s.len());
    let mut chars = s.chars().peekable();
    while let Some(c) = chars.next() {
        if c == '\x1b' {
            while let Some(&next) = chars.peek() {
                chars.next();
                if next == 'm' {
                    break;
                }
            }
        } else {
            result.push(c);
        }
    }
    result
}

fn visible_width(s: &str) -> usize {
    strip_ansi(s).chars().count()
}

fn print_box(title: &str, content: &[String], color: Color) {
    let content_width = content
        .iter()
        .map(|s| visible_width(s))
        .max()
        .unwrap_or(40)
        .max(40);
    let title_str = format!(" {} ", title);
    let title_len = title_str.chars().count();
    let box_width = content_width + 2;

    let top_right_padding = box_width.saturating_sub(title_len + 1);
    println!(
        "{}{}{}{}",
        "╭─".color(color),
        title_str.color(color).bold(),
        "─".repeat(top_right_padding).color(color),
        "╮".color(color)
    );

    for line in content {
        let padding = content_width - visible_width(line);
        println!(
            "{} {}{} {}",
            "│".color(color),
            line,
            " ".repeat(padding),
            "│".color(color)
        );
    }

    println!(
        "{}{}{}",
        "╰".color(color),
        "─".repeat(box_width).color(color),
        "╯".color(color)
    );
}

fn print_usage() {
    println!();
    println!(
        "Usage: {} {}",
        "fatcat".cyan().bold(),
        "[PATH] [OPTIONS]".dimmed()
    );
    println!("Try '{}' for help.", "fatcat --help".green());
    println!();
}

fn print_help() {
    println!();
    println!("{} {}", "fatcat".cyan().bold(), VERSION.dimmed());
    println!(
        "{}",
        "Hunt down the fat files hogging your disk space.".dimmed()
    );
    println!();
    println!(
        "Usage: {} {}",
        "fatcat".cyan().bold(),
        "[PATH] [OPTIONS]".dimmed()
    );
    println!();

    let options = vec![
        format!(
            "{}  {}     Minimum file size in MB (default: 100)",
            "-s, --size".green(),
            "<MB>".dimmed()
        ),
        format!(
            "{}  {}   Save results to log file",
            "-o, --output".green(),
            "<FILE>".dimmed()
        ),
        format!(
            "{}  {}      Show top N files (default: 20)",
            "-t, --top".green(),
            "<N>".dimmed()
        ),
        format!(
            "{}            Show detailed statistics",
            "-v, --verbose".green()
        ),
        format!(
            "{}               Show this help message",
            "-h, --help".green()
        ),
    ];
    print_box("Options", &options, Color::Blue);

    println!();
    let examples = vec![
        "fatcat".to_string(),
        "fatcat /home".to_string(),
        "fatcat ./downloads -s 500".to_string(),
        "fatcat -v -o result.log".to_string(),
    ];
    print_box("Examples", &examples, Color::Cyan);
    println!();
}

fn print_error(msg: &str) {
    println!();
    print_usage();
    let content = vec![msg.to_string()];
    print_box("Error", &content, Color::Red);
    println!();
}

fn scan_directory(
    root: &str,
    min_size_bytes: u64,
    file_count: &AtomicU64,
    dir_count: &AtomicU64,
) -> Vec<FileInfo> {
    let spinner = ProgressBar::new_spinner();
    spinner.set_style(
        ProgressStyle::default_spinner()
            .template("  {spinner:.cyan} {msg}")
            .unwrap()
            .tick_chars("⠋⠙⠹⠸⠼⠴⠦⠧⠇⠏"),
    );
    spinner.set_message("Scanning...");
    spinner.enable_steady_tick(Duration::from_millis(80));

    let mut files: Vec<FileInfo> = Vec::new();

    for entry in WalkDir::new(root)
        .skip_hidden(false)
        .follow_links(false)
        .into_iter()
        .filter_map(|e| e.ok())
    {
        let file_type = entry.file_type();
        if file_type.is_dir() {
            dir_count.fetch_add(1, Ordering::Relaxed);
        } else if file_type.is_file() {
            file_count.fetch_add(1, Ordering::Relaxed);
            if let Ok(metadata) = entry.metadata() {
                let size = metadata.len();
                if size >= min_size_bytes {
                    files.push(FileInfo {
                        path: entry.path(),
                        size,
                    });
                }
            }
        }
    }

    spinner.finish_and_clear();

    files.sort_unstable_by(|a, b| b.size.cmp(&a.size));
    files
}

fn write_log(
    files: &[FileInfo],
    log_path: &str,
    scan_root: &str,
    min_size: u64,
    total_files: u64,
    total_dirs: u64,
    elapsed: f64,
) -> std::io::Result<()> {
    let file = File::create(log_path)?;
    let mut w = BufWriter::new(file);

    let timestamp = Local::now().format("%Y-%m-%d %H:%M:%S");

    writeln!(w, "FATCAT - Scan Report")?;
    writeln!(w, "====================")?;
    writeln!(w)?;
    writeln!(w, "Timestamp       : {}", timestamp)?;
    writeln!(w, "Scan Target     : {}", scan_root)?;
    writeln!(w, "Min Size        : {}", format_size(min_size))?;
    writeln!(w, "Files Scanned   : {}", total_files)?;
    writeln!(w, "Dirs Scanned    : {}", total_dirs)?;
    writeln!(w, "Files Found     : {}", files.len())?;
    writeln!(w, "Elapsed Time    : {:.2} sec", elapsed)?;
    writeln!(w)?;

    let total_size: u64 = files.iter().map(|f| f.size).sum();
    writeln!(w, "Total Size      : {}", format_size(total_size))?;
    writeln!(w)?;

    let gb_files = files.iter().filter(|f| f.size >= 1_073_741_824).count();
    let mb_500_files = files
        .iter()
        .filter(|f| f.size >= 524_288_000 && f.size < 1_073_741_824)
        .count();
    let mb_100_files = files
        .iter()
        .filter(|f| f.size >= 104_857_600 && f.size < 524_288_000)
        .count();

    writeln!(w, "Size Distribution")?;
    writeln!(w, "-----------------")?;
    writeln!(w, ">= 1 GB         : {} files", gb_files)?;
    writeln!(w, "500 MB - 1 GB   : {} files", mb_500_files)?;
    writeln!(w, "100 MB - 500 MB : {} files", mb_100_files)?;
    writeln!(w)?;

    writeln!(w, "All Files (sorted by size)")?;
    writeln!(w, "--------------------------")?;
    for (i, file) in files.iter().enumerate() {
        writeln!(
            w,
            "{:>5}. {:>12}  {}",
            i + 1,
            format_size(file.size),
            file.path.display()
        )?;
    }

    w.flush()?;
    Ok(())
}

struct Config {
    path: String,
    min_size_mb: u64,
    output: Option<String>,
    top_n: usize,
    verbose: bool,
}

fn parse_args() -> Result<Config, String> {
    let args: Vec<String> = env::args().collect();

    let mut config = Config {
        path: String::from("./"),
        min_size_mb: 100,
        output: None,
        top_n: 20,
        verbose: false,
    };

    let mut i = 1;
    while i < args.len() {
        match args[i].as_str() {
            "-h" | "--help" => {
                print_help();
                std::process::exit(0);
            }
            "-s" | "--size" => {
                i += 1;
                if i >= args.len() {
                    return Err(format!(
                        "Option '{}' requires an argument.",
                        "-s, --size".yellow()
                    ));
                }
                config.min_size_mb = args[i]
                    .parse()
                    .map_err(|_| format!("Invalid size value: '{}'", args[i].yellow()))?;
            }
            "-o" | "--output" => {
                i += 1;
                if i >= args.len() {
                    return Err(format!(
                        "Option '{}' requires an argument.",
                        "-o, --output".yellow()
                    ));
                }
                config.output = Some(args[i].clone());
            }
            "-t" | "--top" => {
                i += 1;
                if i >= args.len() {
                    return Err(format!(
                        "Option '{}' requires an argument.",
                        "-t, --top".yellow()
                    ));
                }
                config.top_n = args[i]
                    .parse()
                    .map_err(|_| format!("Invalid number: '{}'", args[i].yellow()))?;
            }
            "-v" | "--verbose" => {
                config.verbose = true;
            }
            arg if arg.starts_with('-') => {
                return Err(format!("Unknown option: '{}'", arg.yellow()));
            }
            arg => {
                config.path = arg.to_string();
            }
        }
        i += 1;
    }

    Ok(config)
}

fn main() {
    let config = match parse_args() {
        Ok(c) => c,
        Err(e) => {
            print_error(&e);
            std::process::exit(1);
        }
    };

    let min_size_bytes = config.min_size_mb * 1024 * 1024;

    println!();
    println!("{} {}", "fatcat".cyan().bold(), VERSION.dimmed());
    println!();
    println!(
        "  {} {}    {} {} MB",
        "Target:".dimmed(),
        config.path.white(),
        "Min:".dimmed(),
        config.min_size_mb.to_string().white()
    );
    println!();

    let start = Instant::now();
    let file_count = AtomicU64::new(0);
    let dir_count = AtomicU64::new(0);

    let files = scan_directory(&config.path, min_size_bytes, &file_count, &dir_count);

    let elapsed = start.elapsed().as_secs_f64();
    let total_files = file_count.load(Ordering::Relaxed);
    let total_dirs = dir_count.load(Ordering::Relaxed);

    println!(
        "  {} {:.2}s  {} {}  {} {}",
        "Done:".green(),
        elapsed,
        "Scanned:".dimmed(),
        total_files,
        "Found:".cyan(),
        files.len()
    );
    println!();

    if config.verbose {
        let gb_count = files.iter().filter(|f| f.size >= 1_073_741_824).count();
        let mb_500_count = files
            .iter()
            .filter(|f| f.size >= 524_288_000 && f.size < 1_073_741_824)
            .count();
        let mb_100_count = files
            .iter()
            .filter(|f| f.size >= 104_857_600 && f.size < 524_288_000)
            .count();
        let total_size: u64 = files.iter().map(|f| f.size).sum();

        let stats = vec![
            format!("Dirs scanned    : {}", total_dirs),
            format!("Total size      : {}", format_size(total_size)),
            format!(">= 1 GB         : {} files", gb_count),
            format!("500 MB - 1 GB   : {} files", mb_500_count),
            format!("100 MB - 500 MB : {} files", mb_100_count),
        ];
        print_box("Statistics", &stats, Color::Magenta);
        println!();
    }

    if !files.is_empty() {
        let display_count = std::cmp::min(config.top_n, files.len());
        let mut file_list: Vec<String> = Vec::with_capacity(display_count);
        for (i, file) in files.iter().take(display_count).enumerate() {
            file_list.push(format!(
                "{:>3}. {:>10}  {}",
                i + 1,
                format_size(file.size),
                file.path.display()
            ));
        }
        print_box(&format!("Top {} Files", display_count), &file_list, Color::Cyan);
        println!();
    } else {
        let content = vec!["No files found matching criteria.".to_string()];
        print_box("Result", &content, Color::Yellow);
        println!();
    }

    if let Some(ref log_path) = config.output {
        match write_log(
            &files,
            log_path,
            &config.path,
            min_size_bytes,
            total_files,
            total_dirs,
            elapsed,
        ) {
            Ok(_) => println!("  {} {}", "Log saved:".green(), log_path),
            Err(e) => println!("  {} {}", "Failed:".red(), e),
        }
        println!();
    }
}