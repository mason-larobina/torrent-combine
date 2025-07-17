use std::fs::{self, File};
use std::io::{self, BufReader, BufWriter, Read, Write};
use std::path::PathBuf;

use log::error;
use tempfile::NamedTempFile;

pub fn process_group(paths: &[PathBuf], basename: &str) -> io::Result<()> {
    let res = check_sanity_and_completes(paths)?;

    if let Some((temp, is_complete)) = res {
        if is_complete.iter().any(|&c| !c) {
            for (j, &complete) in is_complete.iter().enumerate() {
                if !complete {
                    let path = &paths[j];
                    let parent = path.parent().ok_or(io::Error::new(
                        io::ErrorKind::InvalidInput,
                        "No parent directory",
                    ))?;
                    let file_name = path.file_name().unwrap().to_string_lossy().into_owned();
                    let merged_path = parent.join(format!("{}.merged", file_name));
                    let local_temp = NamedTempFile::new_in(parent)?;
                    fs::copy(temp.path(), local_temp.path())?;
                    local_temp.persist(&merged_path)?;
                }
            }
        }
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
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "Size mismatch in group",
            ));
        }
    }

    let temp = NamedTempFile::new()?;
    let file = temp.reopen()?;
    let mut writer = BufWriter::new(file);

    let mut readers: Vec<BufReader<File>> = Vec::with_capacity(paths.len());

    for p in paths {
        readers.push(BufReader::new(File::open(p)?));
    }

    const BUF_SIZE: usize = 8192;

    let mut buffers: Vec<Vec<u8>> = vec![vec![]; paths.len()];

    let mut is_complete = vec![true; paths.len()];

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
    }

    writer.flush()?;

    Ok(Some((temp, is_complete)))
}

