use clap::Parser;
use std::collections::HashMap;
use std::fs;
use std::io;
use std::path::PathBuf;

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

    groups.into_par_iter().for_each(|((basename, _), paths)| {
        if paths.len() >= 2 {
            log::info!("Processing group {} with {} files", basename, paths.len());
            if let Err(e) = merger::process_group(&paths, &basename, args.replace) {
                error!("Error processing group {}: {:?}", basename, e);
            }
        }
    });

    log::info!("Processing completed");
    Ok(())
}
