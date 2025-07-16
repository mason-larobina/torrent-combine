# Torrent File Merger Design

## Overview

This Rust application is designed to merge partially downloaded torrent files within a specified root directory. It targets video files larger than 1MB, performs sanity checks on the data, and merges compatible files by performing a bitwise OR operation on their contents. The merged result is saved with a `.merged` suffix, but only if it differs from the original files.

## Requirements

- Input: A single root directory containing torrent files (potentially partial, pre-allocated with zeros).
- Filter: Process only files larger than 1MB (targeting video files).
- Grouping: Group files by identical filename and filesize (assuming identical contents if names and sizes match).
- Sanity Check: For each byte position in paired files:
  - Both bytes are zero, or
  - One is zero and the other is non-zero, or
  - Both are non-zero but equal.
- Merge Operation: If sanity check passes, perform a bitwise OR on the file contents to create a merged file.
- Output: Save the merged file with a `.merged` suffix in the same directory.
- Optimization: Do not create or persist the merged file if it is identical to one of the input files.
- Error Handling: Skip invalid pairs, log errors, and continue processing.

## Assumptions

- Files with the same name and size have identical contents (no need to verify byte-by-byte for grouping).
- Files are pre-allocated with zeros; partial downloads have correct data in downloaded chunks and zeros elsewhere.
- Targeting video files, but no specific video format checks beyond size filter.
- The application runs on a filesystem that supports large files and binary operations.
- No subdirectories are recursed; only files directly in the root directory are considered (for simplicity, as per the request).

## Functionality

1. **Command-Line Interface**:
   - Accept a single argument: the root directory path.
   - Example: `cargo run -- /path/to/root/dir`

2. **File Discovery**:
   - Scan the root directory for all files.
   - Filter files where size > 1MB.

3. **Grouping Files**:
   - Create a map or groups of files based on (filename, filesize) as the key.
   - Only groups with exactly two files will be considered for merging (extendable if needed).

4. **Sanity Check**:
   - For each pair of files in a group:
     - Open both files in binary mode.
     - Read byte-by-byte (or in chunks for efficiency).
     - For each position:
       - If both bytes == 0, valid.
       - If one == 0 and the other != 0, valid.
       - If both != 0 and equal, valid.
       - Otherwise, invalid pair; skip and log.
   - If the entire pair passes, proceed to merge.

5. **Merging**:
   - Create a new file with the same name but `.merged` suffix.
   - For each byte position, write the bitwise OR of the two bytes.
   - After merging, compare the merged file's contents with each original:
     - If identical to one, delete the merged file.
   - Note: Since files can be large, perform comparisons and operations in a memory-efficient way (e.g., streaming).

6. **Edge Cases**:
   - Single file in a group: Skip, no merge needed.
   - More than two files: For simplicity, handle pairs or extend to multi-file OR.
   - Files smaller than 1MB: Ignore.
   - Identical files: Merged result same as original; don't persist.
   - Mismatch in size (though grouped by size): Error.
   - I/O errors: Handle gracefully, log, and continue.

## Implementation Plan

- **Language**: Rust (using standard library for file I/O, no external crates initially for simplicity).
- **Structure**:
  - `main.rs`: CLI parsing, directory scanning, grouping, and orchestration.
  - `merger.rs`: Functions for sanity check and merging.
  - Use `std::fs` and `std::io` for file operations.
  - For efficiency with large files: Read/write in buffered chunks (e.g., 4KB buffers).
- **Error Handling**: Use `Result` types, log to stderr.
- **Testing**: Unit tests for sanity check and OR logic; integration tests with sample files.
- **Extensions**: Later add recursion into subdirectories, multi-file merging, or progress reporting.

## Potential Challenges

- Handling very large files without excessive memory use.
- Ensuring atomicity when writing merged files (e.g., write to temp file then rename).
- Performance: Optimize byte-wise operations for speed.

This design provides a foundation for the application. If requirements change (e.g., handling subdirectories or more than two files), the design can be updated accordingly.
