//! `tpt-inference` CLI: sample logs → suggested `.tpt-log` schema.
//!
//! Usage:
//! ```text
//! tpt-inference [--provider claude|openai|openrouter|grok|ollama|mock] [--retries N] [sample.log ...]
//! cat sample.log | tpt-inference --provider openai
//! ```
//!
//! Samples are read from the given files (or stdin if none). The provider is
//! chosen by `--provider` or the `TPT_PROVIDER` env var (default `mock`). Live
//! providers require the relevant API key environment variable.

use std::io::Read;
use tpt_inference::{infer_schema, provider_by_name};

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let mut provider_name = std::env::var("TPT_PROVIDER").unwrap_or_else(|_| "mock".into());
    let mut retries: usize = 3;
    let mut files: Vec<String> = Vec::new();

    let mut i = 1;
    while i < args.len() {
        match args[i].as_str() {
            "--provider" => {
                i += 1;
                if let Some(v) = args.get(i) {
                    provider_name = v.clone();
                }
            }
            "--retries" => {
                i += 1;
                if let Some(v) = args.get(i).and_then(|s| s.parse().ok()) {
                    retries = v;
                }
            }
            "--help" | "-h" => {
                print_help();
                return;
            }
            other => files.push(other.to_string()),
        }
        i += 1;
    }

    let provider = match provider_by_name(&provider_name) {
        Ok(p) => p,
        Err(e) => {
            eprintln!("error: {e}");
            std::process::exit(2);
        }
    };

    let samples = match read_samples(&files) {
        Ok(s) => s,
        Err(e) => {
            eprintln!("error reading samples: {e}");
            std::process::exit(1);
        }
    };
    if samples.is_empty() {
        eprintln!("no log samples provided (pass file paths or pipe via stdin)");
        std::process::exit(1);
    }

    let sample_refs: Vec<&str> = samples.iter().map(|s| s.as_str()).collect();
    match infer_schema(provider.as_ref(), &sample_refs, retries) {
        Ok(schema) => {
            println!("{schema}");
        }
        Err(e) => {
            eprintln!("inference failed: {e}");
            std::process::exit(1);
        }
    }
}

fn read_samples(files: &[String]) -> std::io::Result<Vec<String>> {
    let mut out = Vec::new();
    if files.is_empty() {
        let mut buf = String::new();
        std::io::stdin().read_to_string(&mut buf)?;
        out.push(buf);
    } else {
        for f in files {
            out.push(std::fs::read_to_string(f)?);
        }
    }
    // Split concatenated text into individual lines as separate samples.
    let lines: Vec<String> = out
        .iter()
        .flat_map(|s| s.lines().map(|l| l.to_string()))
        .filter(|l| !l.trim().is_empty())
        .collect();
    Ok(if lines.is_empty() { out } else { lines })
}

fn print_help() {
    println!(
        "tpt-inference - suggest a .tpt-log schema from raw log samples\n\
         \n\
         Usage:\n\
           tpt-inference [--provider NAME] [--retries N] [file ...]\n\
           cat samples.log | tpt-inference --provider openai\n\
         \n\
         Providers: claude, openai, openrouter, grok, ollama, mock\n\
         Provider selection falls back to the TPT_PROVIDER env var (default mock)."
    );
}
