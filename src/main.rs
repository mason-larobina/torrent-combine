use clap::Parser;
use std::collections::HashMap;
use std::fs;
use std::io;
use std::path::PathBuf;

mod merger;

#[derive(Parser, Debug)]
#[command(name = "torrent-combine")]
struct Args {
    root_dir: PathBuf,
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
    let args = Args::parse();

    let files = collect_large_files(&args.root_dir)?;

    let mut groups: HashMap<(String, u64), Vec<PathBuf>> = HashMap::new();
    for file in files {
        if let Some(basename) = file.file_name().map(|s| s.to_string_lossy().to_string()) {
            if let Ok(metadata) = fs::metadata(&file) {
                let size = metadata.len();
                groups.entry((basename, size)).or_insert(Vec::new()).push(file);
            }
        }
    }

    for ((basename, size), paths) in groups {
        if paths.len() >= 2 {
            if let Err(e) = merger::process_group(&paths, &basename) {
                eprintln!("Error processing group {}: {:?}", basename, e);
            }
        }
    }

    Ok(())
}
