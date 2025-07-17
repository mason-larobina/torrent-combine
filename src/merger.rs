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
                        log::debug!("Created merged file {:?} for incomplete original {:?}", merged_path, path);
                    }
                }
            }
        } else {
            log::debug!(
                "All files in group {} are complete, no {} needed",
                basename,
                if replace { "replacements" } else { "merged files created" }
            );
        }
        log::info!(
            "Completed {} for group {}",
            if replace { "replacement" } else { "merge" },
            basename
        );
    } else {
        error!("Failed sanity check for group: {}", basename);
    }

    Ok(())
}

fn check_sanity_and_completes(paths: &[PathBuf]) -> io::Result<Option<(NamedTempFile, Vec<bool>)>> {
    if paths.is_empty() {
        return Ok(Some((NamedTempFile::new()?, vec![])));
    }

    let size = fs::metadata(&paths[0])?.len();

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

    let mut buffers: Vec<Vec<u8>> = vec![vec![]; paths.len()];

    let mut is_complete = vec![true; paths.len()];

    let mut processed = 0u64;
    for offset in (0..size).step_by(BUF_SIZE) {
        let chunk_size = ((size - offset) as usize).min(BUF_SIZE);

        for (i, reader) in readers.iter_mut().enumerate() {
            let mut buf = vec![0; chunk_size];
            reader.read_exact(&mut buf)?;
            buffers[i] = buf;
        }

        let mut or_chunk = vec![0u8; chunk_size];
        for pos in 0..chunk_size {
            let mut or_byte = 0u8;
            let mut non_zero_val: Option<u8> = None;

            for i in 0..paths.len() {
                let b = buffers[i][pos];
                or_byte |= b;
                if b != 0 {
                    match non_zero_val {
                        None => non_zero_val = Some(b),
                        Some(v) if v != b => return Ok(None),
                        _ => {}
                    }
                }
            }

            or_chunk[pos] = or_byte;

            for i in 0..paths.len() {
                if buffers[i][pos] != or_byte {
                    is_complete[i] = false;
                }
            }
        }

        writer.write_all(&or_chunk)?;

        processed += chunk_size as u64;
        if processed % (BUF_SIZE as u64 * 100) == 0 {
            log::debug!("Processed {} of {} bytes for group", processed, size);
        }
    }

    if processed % (BUF_SIZE as u64 * 100) != 0 {
        log::debug!("Processed {} of {} bytes for group", processed, size);
    }

    writer.flush()?;

    Ok(Some((temp, is_complete)))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use std::io::self;
    use tempfile::tempdir;

    #[test]
    fn test_empty_group() -> io::Result<()> {
        if let Some((temp, is_complete)) = check_sanity_and_completes(&[])? {
            assert_eq!(is_complete, vec![]);
            assert_eq!(fs::read(temp.path())?, vec![]);
        } else {
            panic!("Expected Some for empty group");
        }
        Ok(())
    }

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

