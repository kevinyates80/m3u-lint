use std::env;
use std::fs;
use std::process::ExitCode;

use m3u_lint::{parse, ParseOptions};

fn main() -> ExitCode {
    let mut lenient = false;
    let mut path = None;

    for arg in env::args().skip(1) {
        match arg.as_str() {
            "--lenient" => lenient = true,
            "-h" | "--help" => {
                print_usage();
                return ExitCode::SUCCESS;
            }
            other => {
                if path.is_some() {
                    eprintln!("unexpected extra argument: {other}");
                    return ExitCode::FAILURE;
                }
                path = Some(other.to_string());
            }
        }
    }

    let path = match path {
        Some(p) => p,
        None => {
            print_usage();
            return ExitCode::FAILURE;
        }
    };

    let contents = match fs::read_to_string(&path) {
        Ok(c) => c,
        Err(e) => {
            eprintln!("{path}: {e}");
            return ExitCode::FAILURE;
        }
    };

    let opts = ParseOptions { lenient };
    match parse(&contents, &opts) {
        Ok(playlist) => {
            println!("{path}: {} tracks", playlist.entries.len());
            match playlist.total_duration_secs() {
                Some(secs) => println!("total duration: {}", format_duration(secs)),
                None => println!("total duration: unknown (some tracks missing length)"),
            }
            ExitCode::SUCCESS
        }
        Err(e) => {
            eprintln!("{path}: {e}");
            if !lenient {
                eprintln!("(retry with --lenient to parse past this)");
            }
            ExitCode::FAILURE
        }
    }
}

fn format_duration(total_secs: i64) -> String {
    let h = total_secs / 3600;
    let m = (total_secs % 3600) / 60;
    let s = total_secs % 60;
    if h > 0 {
        format!("{h}h {m}m {s}s")
    } else {
        format!("{m}m {s}s")
    }
}

fn print_usage() {
    eprintln!("usage: m3u-lint [--lenient] <playlist.m3u>");
}
