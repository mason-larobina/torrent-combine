use std::fs::{self, File};
use std::io::{self, BufReader, BufWriter, Read, Write};
use std::path::PathBuf;

use log::error;
use tempfile::NamedTempFile;

pub fn process_group(paths: &[PathBuf], basename: &str, replace: bool) -> io::Result<()> {
    log::debug!("Processing paths for group {}: {:?}", basename, paths);

    let res = check_sanity_and_completes(paths)?;

    if let Some((temp, is_complete)) = res {
        log::info!("Sanity check passed for group {}", basename);

        let any_incomplete = is_complete.iter().any(|&c| !c);
        if any_incomplete {
            for (j, &complete) in is_complete.iter().enumerate() {
                if !complete {
                    let path = &paths[j];
                    let parent = path.parent().ok_or(io::Error::new(
                        io::ErrorKind::InvalidInput,
                        "No parent directory",
                    ))?;
                    let local_temp = NamedTempFile::new_in(parent)?;
                    fs::copy(temp.path(), local_temp.path())?;
                    if replace {
                        fs::rename(local_temp.path(), path)?;
                        log::debug!("Replaced original {:?} with merged content", path);
                    } else {
                        let file_name = path.file_name().unwrap().to_string_lossy().into_owned();
                        let merged_path = parent.join(format!("{}.merged", file_name));
                        local_temp.persist(&merged_path)?;
                        log::debug!(
                            "Created merged file {:?} for incomplete original {:?}",
                            merged_path,
                            path
                        );
                    }
                }
            }
            log::info!(
                "Completed {} for group {}",
                if replace { "replacement" } else { "merge" },
                basename
            );
        } else {
            log::info!(
                "Skipped group {} (all complete, no action needed)",
                basename
            );
        }
    } else {
        error!("Failed sanity check for group: {}", basename);
    }

    Ok(())
}

fn check_word_sanity(w: u64, or_w: u64) -> bool {
    if w == or_w {
        return true;
    }
    for k in 0..8 {
        let shift = k * 8;
        let b = (w >> shift) as u8;
        let or_b = (or_w >> shift) as u8;
        if b != 0 && b != or_b {
            return false;
        }
    }
    true
}

fn check_sanity_and_completes(paths: &[PathBuf]) -> io::Result<Option<(NamedTempFile, Vec<bool>)>> {
    if paths.is_empty() {
        return Ok(None);
    }

    let size = fs::metadata(&paths[0])?.len();
    if size == 0 {
        return Ok(None);
    }

    for p in &paths[1..] {
        if fs::metadata(p)?.len() != size {
            log::error!("Size mismatch in group for path {:?}", p);
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "Size mismatch in group",
            ));
        }
    }

    log::debug!("Checking sanity for {} files of size {}", paths.len(), size);

    let temp_dir = paths[0].parent().ok_or(io::Error::new(
        io::ErrorKind::InvalidInput,
        "No parent directory for first path",
    ))?;
    let temp = NamedTempFile::new_in(temp_dir)?;
    let file = temp.reopen()?;
    let mut writer = BufWriter::new(file);

    let mut readers: Vec<BufReader<File>> = Vec::with_capacity(paths.len());
    for p in paths {
        readers.push(BufReader::new(File::open(p)?));
    }

    const BUF_SIZE: usize = 8192;
    let mut buffers: Vec<Vec<u8>> = (0..paths.len()).map(|_| vec![0; BUF_SIZE]).collect();
    let mut is_complete = vec![true; paths.len()];
    let mut or_chunk = vec![0; BUF_SIZE];

    let mut processed = 0u64;
    while processed < size {
        let chunk_size = ((size - processed) as usize).min(BUF_SIZE);
        let buffers_slice = &mut buffers;
        let or_chunk_slice = &mut or_chunk[..chunk_size];

        for (i, reader) in readers.iter_mut().enumerate() {
            reader.read_exact(&mut buffers_slice[i][..chunk_size])?;
        }

        or_chunk_slice.copy_from_slice(&buffers_slice[0][..chunk_size]);

        let or_chunk_ptr = or_chunk_slice.as_ptr();
        let (prefix, words, suffix) = unsafe { or_chunk_slice.align_to_mut::<u64>() };

        for b in prefix.iter_mut() {
            let offset = (b as *const u8 as usize) - (or_chunk_ptr as usize);
            for i in 1..paths.len() {
                *b |= buffers_slice[i][offset];
            }
        }
        for (j, w) in words.iter_mut().enumerate() {
            for i in 1..paths.len() {
                let (_, other_words, _) = unsafe { buffers_slice[i][..chunk_size].align_to::<u64>() };
                *w |= other_words[j];
            }
        }
        for b in suffix.iter_mut() {
            let offset = (b as *const u8 as usize) - (or_chunk_ptr as usize);
            for i in 1..paths.len() {
                *b |= buffers_slice[i][offset];
            }
        }

        for i in 0..paths.len() {
            let buffer_slice = &buffers_slice[i][..chunk_size];
            if buffer_slice != or_chunk_slice {
                is_complete[i] = false;
                let (prefix, words, suffix) = unsafe { buffer_slice.align_to::<u64>() };
                let (or_prefix, or_words, or_suffix) = unsafe { or_chunk_slice.align_to::<u64>() };

                if !prefix.iter().zip(or_prefix.iter()).all(|(b, or_b)| *b == 0 || *b == *or_b) {
                    return Ok(None);
                }
                if !words.iter().zip(or_words.iter()).all(|(w, or_w)| check_word_sanity(*w, *or_w)) {
                    return Ok(None);
                }
                if !suffix.iter().zip(or_suffix.iter()).all(|(b, or_b)| *b == 0 || *b == *or_b) {
                    return Ok(None);
                }
            }
        }

        writer.write_all(or_chunk_slice)?;
        processed += chunk_size as u64;
    }

    log::debug!("Processed {} of {} bytes for group", processed, size);
    writer.flush()?;
    Ok(Some((temp, is_complete)))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use std::io;
    use tempfile::tempdir;

    #[test]
    fn test_single_file() -> io::Result<()> {
        let dir = tempdir()?;
        let p1 = dir.path().join("a");
        let data = vec![1u8, 2, 3];
        fs::write(&p1, &data)?;

        let paths = vec![p1];

        if let Some((temp, is_complete)) = check_sanity_and_completes(&paths)? {
            assert_eq!(is_complete, vec![true]);
            assert_eq!(fs::read(temp.path())?, data);
        } else {
            panic!("Expected Some for single file");
        }
        Ok(())
    }

    #[test]
    fn test_size_mismatch() -> io::Result<()> {
        let dir = tempdir()?;
        let p1 = dir.path().join("a");
        fs::write(&p1, vec![1u8, 2, 3])?;

        let p2 = dir.path().join("b");
        fs::write(&p2, vec![4u8, 5])?;

        let paths = vec![p1, p2];
        let res = check_sanity_and_completes(&paths);
        assert!(res.is_err());
        Ok(())
    }

    #[test]
    fn test_sanity_fail() -> io::Result<()> {
        let dir = tempdir()?;
        let p1 = dir.path().join("a");
        fs::write(&p1, vec![1u8, 0])?;

        let p2 = dir.path().join("b");
        fs::write(&p2, vec![2u8, 0])?;

        let paths = vec![p1, p2];
        let res = check_sanity_and_completes(&paths)?;
        assert!(res.is_none());
        Ok(())
    }

    #[test]
    fn test_compatible_merge_multiple() -> io::Result<()> {
        let dir = tempdir()?;
        let p1 = dir.path().join("a");
        let data1 = vec![1u8, 0, 0];
        fs::write(&p1, &data1)?;

        let p2 = dir.path().join("b");
        let data2 = vec![0u8, 1, 0];
        fs::write(&p2, &data2)?;

        let p3 = dir.path().join("c");
        let data3 = vec![1u8, 1, 0];
        fs::write(&p3, &data3)?;

        let paths = vec![p1, p2, p3];

        if let Some((temp, is_complete)) = check_sanity_and_completes(&paths)? {
            assert_eq!(is_complete, vec![false, false, true]);
            assert_eq!(fs::read(temp.path())?, vec![1u8, 1, 0]);
        } else {
            panic!("Expected Some for compatible merge");
        }
        Ok(())
    }

    #[test]
    fn test_process_group_creates_merged_for_incomplete() -> io::Result<()> {
        let dir = tempdir()?;
        let sub1 = dir.path().join("sub1");
        fs::create_dir(&sub1)?;
        let file1 = sub1.join("video.mkv");
        let data_incomplete = vec![0u8, 0, 0];
        fs::write(&file1, &data_incomplete)?;

        let sub2 = dir.path().join("sub2");
        fs::create_dir(&sub2)?;
        let file2 = sub2.join("video.mkv");
        let data_complete = vec![4u8, 5, 6];
        fs::write(&file2, &data_complete)?;

        let paths = vec![file1.clone(), file2.clone()];
        process_group(&paths, "video.mkv", false)?;

        let merged1 = sub1.join("video.mkv.merged");
        assert!(merged1.exists());
        assert_eq!(fs::read(&merged1)?, data_complete);

        let merged2 = sub2.join("video.mkv.merged");
        assert!(!merged2.exists());
        Ok(())
    }

    #[test]
    fn test_process_group_no_merged_on_conflict() -> io::Result<()> {
        let dir = tempdir()?;
        let p1 = dir.path().join("a");
        fs::write(&p1, vec![1u8, 0])?;

        let p2 = dir.path().join("b");
        fs::write(&p2, vec![2u8, 0])?;

        let paths = vec![p1.clone(), p2.clone()];
        process_group(&paths, "dummy", false)?;

        let merged1 = dir.path().join("a.merged");
        assert!(!merged1.exists());

        let merged2 = dir.path().join("b.merged");
        assert!(!merged2.exists());
        Ok(())
    }

    #[test]
    fn test_process_group_no_merged_all_complete() -> io::Result<()> {
        let dir = tempdir()?;
        let p1 = dir.path().join("a");
        let data = vec![4u8, 5, 6];
        fs::write(&p1, &data)?;

        let p2 = dir.path().join("b");
        fs::write(&p2, &data)?;

        let paths = vec![p1.clone(), p2.clone()];
        process_group(&paths, "dummy", false)?;

        let merged1 = dir.path().join("a.merged");
        assert!(!merged1.exists());

        let merged2 = dir.path().join("b.merged");
        assert!(!merged2.exists());
        Ok(())
    }

    #[test]
    fn test_process_group_replace_for_incomplete() -> io::Result<()> {
        let dir = tempdir()?;
        let sub1 = dir.path().join("sub1");
        fs::create_dir(&sub1)?;
        let file1 = sub1.join("video.mkv");
        let data_incomplete = vec![0u8, 0, 0];
        fs::write(&file1, &data_incomplete)?;

        let sub2 = dir.path().join("sub2");
        fs::create_dir(&sub2)?;
        let file2 = sub2.join("video.mkv");
        let data_complete = vec![4u8, 5, 6];
        fs::write(&file2, &data_complete)?;

        let paths = vec![file1.clone(), file2.clone()];
        process_group(&paths, "video.mkv", true)?;

        assert_eq!(fs::read(&file1)?, data_complete);
        assert_eq!(fs::read(&file2)?, data_complete);

        let merged1 = sub1.join("video.mkv.merged");
        assert!(!merged1.exists());

        let merged2 = sub2.join("video.mkv.merged");
        assert!(!merged2.exists());
        Ok(())
    }
}
