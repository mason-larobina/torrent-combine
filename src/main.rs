use clap::Parser;
use std::collections::HashMap;
use std::fs;
use std::io;
use std::path::PathBuf;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;

use log::error;
use rayon::prelude::*;

mod merger;

#[derive(Parser, Debug)]
#[command(name = "torrent-combine")]
struct Args {
    root_dir: PathBuf,
    #[arg(long)]
    replace: bool,
    #[arg(long)]
    num_threads: Option<usize>,
}

fn collect_large_files(dir: &PathBuf) -> io::Result<Vec<PathBuf>> {
    let mut files = Vec::new();
    let mut dirs = vec![dir.clone()];

    while let Some(current_dir) = dirs.pop() {
        for entry in fs::read_dir(&current_dir)? {
            let entry = entry?;
            let path = entry.path();
            if path.is_dir() {
                dirs.push(path);
            } else if let Ok(metadata) = fs::metadata(&path) {
                if metadata.len() > 1_048_576 {
                    files.push(path);
                }
            }
        }
    }

    Ok(files)
}

fn main() -> io::Result<()> {
    if std::env::var("RUST_LOG").is_err() {
        unsafe { std::env::set_var("RUST_LOG", "info") };
    }
    env_logger::init();

    let args = Args::parse();
    log::info!("Processing root directory: {:?}", args.root_dir);

    if let Some(num_threads) = args.num_threads {
        rayon::ThreadPoolBuilder::new().num_threads(num_threads).build_global().unwrap();
    }

    let files = collect_large_files(&args.root_dir)?;
    log::info!("Found {} large files", files.len());

    let mut groups: HashMap<(String, u64), Vec<PathBuf>> = HashMap::new();
    for file in files {
        if let Some(basename) = file.file_name().map(|s| s.to_string_lossy().to_string()) {
            if let Ok(metadata) = fs::metadata(&file) {
                let size = metadata.len();
                groups
                    .entry((basename, size))
                    .or_insert(Vec::new())
                    .push(file);
            }
        }
    }

    let groups_to_process: Vec<_> = groups
        .into_iter()
        .filter(|(_, paths)| paths.len() >= 2)
        .collect();
    let total_groups = groups_to_process.len();
    log::info!("Found {} groups to process", total_groups);

    let groups_processed = Arc::new(AtomicUsize::new(0));
    let merged_groups_count = Arc::new(AtomicUsize::new(0));
    let skipped_groups_count = Arc::new(AtomicUsize::new(0));

    groups_to_process
        .into_par_iter()
        .for_each(|((basename, _), paths)| {
            let groups_processed_cloned = Arc::clone(&groups_processed);
            let merged_groups_count_cloned = Arc::clone(&merged_groups_count);
            let skipped_groups_count_cloned = Arc::clone(&skipped_groups_count);

            match merger::process_group(&paths, &basename, args.replace) {
                Ok(stats) => {
                    let processed_count =
                        groups_processed_cloned.fetch_add(1, Ordering::SeqCst) + 1;
                    let percentage_complete = (processed_count as f64 / total_groups as f64) * 100.0;

                    match stats.status {
                        merger::GroupStatus::Merged => {
                            merged_groups_count_cloned.fetch_add(1, Ordering::SeqCst);
                            let mb_per_sec = (stats.bytes_processed as f64 / 1_048_576.0)
                                / stats.processing_time.as_secs_f64();
                            log::info!(
                                "[{}/{}] Group '{}' merged at {:.2} MB/s. {:.1}% complete.",
                                processed_count,
                                total_groups,
                                basename,
                                mb_per_sec,
                                percentage_complete
                            );
                            if !stats.merged_files.is_empty() {
                                for file in stats.merged_files {
                                    log::info!("  -> Created merged file: {}", file.display());
                                }
                            }
                        }
                        merger::GroupStatus::Skipped => {
                            skipped_groups_count_cloned.fetch_add(1, Ordering::SeqCst);
                            log::info!(
                                "[{}/{}] Group '{}' skipped (all files complete). {:.1}% complete.",
                                processed_count,
                                total_groups,
                                basename,
                                percentage_complete
                            );
                        }
                        merger::GroupStatus::Failed => {
                            log::warn!(
                                "[{}/{}] Group '{}' failed sanity check. {:.1}% complete.",
                                processed_count,
                                total_groups,
                                basename,
                                percentage_complete
                            );
                        }
                    }
                }
                Err(e) => {
                    error!("Error processing group {}: {:?}", basename, e);
                }
            }
        });

    let final_processed = groups_processed.load(Ordering::SeqCst);
    let final_merged = merged_groups_count.load(Ordering::SeqCst);
    let final_skipped = skipped_groups_count.load(Ordering::SeqCst);

    log::info!("--------------------");
    log::info!("Processing Summary:");
    log::info!("Total groups: {}", total_groups);
    log::info!("  - Processed: {}", final_processed);
    log::info!("  - Merged: {}", final_merged);
    log::info!("  - Skipped: {}", final_skipped);
    log::info!("--------------------");
    Ok(())
}
