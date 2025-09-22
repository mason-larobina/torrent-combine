use clap::{Parser, ValueEnum};
use std::collections::HashMap;
use std::fs;
use std::io;
use std::path::PathBuf;
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};

use log::error;
use rayon::prelude::*;

mod merger;

#[derive(Debug, Clone, ValueEnum)]
enum DedupKey {
    #[value(name = "filename-and-size")]
    FilenameAndSize,
    #[value(name = "size-only")]
    SizeOnly,
}

#[derive(Debug, PartialEq, Eq, Hash)]
enum GroupKey {
    FilenameAndSize(String, u64),
    SizeOnly(u64),
}

#[derive(Parser, Debug)]
#[command(name = "torrent-combine")]
struct Args {
    root_dir: PathBuf,
    #[arg(long)]
    replace: bool,
    #[arg(long)]
    num_threads: Option<usize>,
    #[arg(long, value_enum, default_value = "filename-and-size")]
    dedup_mode: DedupKey,
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
        rayon::ThreadPoolBuilder::new()
            .num_threads(num_threads)
            .build_global()
            .unwrap();
    }

    let files = collect_large_files(&args.root_dir)?;
    log::info!("Found {} large files", files.len());

    let mut groups: HashMap<GroupKey, Vec<PathBuf>> = HashMap::new();
    for file in files {
        if let Ok(metadata) = fs::metadata(&file) {
            let size = metadata.len();
            let key = match args.dedup_mode {
                DedupKey::FilenameAndSize => {
                    if let Some(basename) =
                        file.file_name().map(|s| s.to_string_lossy().to_string())
                    {
                        GroupKey::FilenameAndSize(basename, size)
                    } else {
                        continue;
                    }
                }
                DedupKey::SizeOnly => GroupKey::SizeOnly(size),
            };
            groups.entry(key).or_insert(Vec::new()).push(file);
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
        .for_each(|(group_key, paths)| {
            let groups_processed_cloned = Arc::clone(&groups_processed);
            let merged_groups_count_cloned = Arc::clone(&merged_groups_count);
            let skipped_groups_count_cloned = Arc::clone(&skipped_groups_count);

            let group_name = match &group_key {
                GroupKey::FilenameAndSize(basename, size) => format!("{}@{}", basename, size),
                GroupKey::SizeOnly(size) => format!("size-{}", size),
            };

            match merger::process_group(&paths, &group_name, args.replace) {
                Ok(stats) => {
                    let processed_count =
                        groups_processed_cloned.fetch_add(1, Ordering::SeqCst) + 1;
                    let percentage_complete =
                        (processed_count as f64 / total_groups as f64) * 100.0;

                    match stats.status {
                        merger::GroupStatus::Merged => {
                            merged_groups_count_cloned.fetch_add(1, Ordering::SeqCst);
                            let mb_per_sec = (stats.bytes_processed as f64 / 1_048_576.0)
                                / stats.processing_time.as_secs_f64();
                            log::info!(
                                "[{}/{}] Group '{}' merged at {:.2} MB/s. {:.1}% complete.",
                                processed_count,
                                total_groups,
                                group_name,
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
                                group_name,
                                percentage_complete
                            );
                        }
                        merger::GroupStatus::Failed => {
                            log::warn!(
                                "[{}/{}] Group '{}' failed sanity check. {:.1}% complete.",
                                processed_count,
                                total_groups,
                                group_name,
                                percentage_complete
                            );
                        }
                    }
                }
                Err(e) => {
                    error!("Error processing group {}: {:?}", group_name, e);
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

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;

    #[test]
    fn test_dedup_key_enum_variants() {
        assert_eq!(
            format!("{:?}", DedupKey::FilenameAndSize),
            "FilenameAndSize"
        );
        assert_eq!(format!("{:?}", DedupKey::SizeOnly), "SizeOnly");
    }

    #[test]
    fn test_group_key_equality() {
        let key1 = GroupKey::FilenameAndSize("test.mkv".to_string(), 1024);
        let key2 = GroupKey::FilenameAndSize("test.mkv".to_string(), 1024);
        let key3 = GroupKey::FilenameAndSize("other.mkv".to_string(), 1024);
        let key4 = GroupKey::SizeOnly(1024);
        let key5 = GroupKey::SizeOnly(1024);
        let key6 = GroupKey::SizeOnly(2048);

        assert_eq!(key1, key2);
        assert_ne!(key1, key3);
        assert_ne!(key1, key4);
        assert_eq!(key4, key5);
        assert_ne!(key4, key6);
    }

    #[test]
    fn test_group_key_hash() {
        let mut map: HashMap<GroupKey, Vec<PathBuf>> = HashMap::new();

        let key1 = GroupKey::FilenameAndSize("test.mkv".to_string(), 1024);
        let key2 = GroupKey::SizeOnly(1024);

        map.insert(key1, vec![PathBuf::from("/path1")]);
        map.insert(key2, vec![PathBuf::from("/path2")]);

        assert_eq!(map.len(), 2);

        let key1_dup = GroupKey::FilenameAndSize("test.mkv".to_string(), 1024);
        map.entry(key1_dup)
            .or_insert(Vec::new())
            .push(PathBuf::from("/path3"));

        assert_eq!(map.len(), 2);
    }

    #[test]
    fn test_group_name_formatting() {
        let key1 = GroupKey::FilenameAndSize("video.mkv".to_string(), 2097152);
        let key2 = GroupKey::SizeOnly(1048576);

        let name1 = match &key1 {
            GroupKey::FilenameAndSize(basename, size) => format!("{}@{}", basename, size),
            GroupKey::SizeOnly(size) => format!("size-{}", size),
        };

        let name2 = match &key2 {
            GroupKey::FilenameAndSize(basename, size) => format!("{}@{}", basename, size),
            GroupKey::SizeOnly(size) => format!("size-{}", size),
        };

        assert_eq!(name1, "video.mkv@2097152");
        assert_eq!(name2, "size-1048576");
    }
}
