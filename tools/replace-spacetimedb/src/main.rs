#![allow(clippy::disallowed_macros)]
use clap::Parser;
use replace_spacetimedb::{replace_in_tree, ReplaceOptions};

/// Replace all occurrences of "spacetimedb" under <target_dir> with <replacement>.
#[derive(Parser, Debug)]
#[command(version, about)]
struct Args {
    /// Directory to process (recursively).
    target_dir: String,
    /// Replacement string for 'spacetimedb' in index files.
    index_replacement: String,
    /// Replacement string for 'spacetimedb' in other files.
    other_replacement: String,

    /// Only process given file extensions (comma-separated, e.g. "ts,tsx,js,json").
    #[arg(long)]
    only_exts: Option<String>,

    /// Follow symlinks.
    #[arg(long)]
    follow_symlinks: bool,

    /// Include hidden files/dirs.
    #[arg(long)]
    include_hidden: bool,

    /// Ignore globs to skip (can be used multiple times).
    #[arg(long)]
    ignore: Vec<String>,

    /// Dry run: show changes without writing.
    #[arg(long)]
    dry_run: bool,
}

fn main() {
    let args = Args::parse();
    let only_exts = args.only_exts.map(|s| {
        s.split(',')
            .map(|e| e.trim().trim_start_matches('.').to_string())
            .filter(|e| !e.is_empty())
            .collect::<Vec<_>>()
    });

    let opts = ReplaceOptions {
        dry_run: args.dry_run,
        only_exts,
        follow_symlinks: args.follow_symlinks,
        include_hidden: args.include_hidden,
        ignore_globs: args.ignore,
    };

    match replace_in_tree(
        &args.target_dir,
        &args.index_replacement,
        &args.other_replacement,
        &opts,
    ) {
        Ok(stats) => {
            println!(
                "✅ Replacement complete. Files changed: {} | Occurrences: {}",
                stats.files_changed, stats.occurrences
            );
        }
        Err(e) => {
            eprintln!("error: {e}");
            std::process::exit(1);
        }
    }
}
