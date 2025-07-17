# Torrent File Merger Design

## Overview

This Rust application is designed to merge partially downloaded torrent files within a specified root directory. It targets video files larger than 1MB, performs sanity checks on the data, and merges compatible files by performing a bitwise OR operation on their contents. The merged result is saved with a `.merged` suffix, but only if it differs from the original files.

## Requirements

- Input: A single root directory containing torrent files (potentially partial, pre-allocated with zeros).
- Filter: Process only files larger than 1MB (targeting video files).
- Grouping: Group files by identical filename and filesize (assuming identical contents if names and sizes match).
- Sanity Check: For each byte position across all files in the group:
  - Collect non-zero bytes; if any, they must all be equal (zeros are allowed anywhere).
- Merge Operation: If sanity check passes, perform a bitwise OR on the file contents across all files in the group to create merged content.
- Output: Save the merged file with a `.merged` suffix in the same directory.
- Optimization: Do not create or persist the merged file if it is identical to one of the input files.
- Error Handling: Skip invalid groups, log errors, and continue processing.
- Mode: Optional --replace to replace incomplete original files with merged content instead of creating .merged files.

## Assumptions

- Files with the same name and size are expected to be compatible partial versions (zeros in undownloaded chunks), with non-zero bytes matching where present.
- Files are pre-allocated with zeros; partial downloads have correct data in downloaded chunks and zeros elsewhere.
- Targeting video files, but no specific video format checks beyond size filter.
- Subdirectories are recursed to find files across the directory tree.
- Files with the same basename and size are candidates for grouping; during merging, non-zero contents are checked for consistency.

## Functionality

1. **Command-Line Interface**:
   - Accept a single argument: the root directory path.
   - Optional --replace flag.
   - Example: `cargo run -- /path/to/root/dir`
   - Example with replace: `cargo run -- /path/to/root/dir --replace`

2. **File Discovery**:
   - Recursively scan the root directory and all subdirectories for files.
   - Filter files where size > 1MB.

3. **Grouping Files**:
   - Create a map or groups of files based on (basename, filesize) as the key.
   - Groups with at least two files will be considered for merging.

4. **Sanity Check**:
   - For each group with >=2 files:
     - Open all files in binary mode.
     - Read byte-by-byte (or in chunks for efficiency) across all files.
     - For each position:
       - Collect non-zero bytes from all files.
       - If there are non-zero bytes, they must all be equal.
       - If conflicting non-zero values, invalid group; skip and log.
   - Extends to groups of >2 files by checking consistency across all bytes at each position.
   - If the entire group passes, proceed to merge.

5. **Merging**:
   - Compute the merged contents by performing a bitwise OR on the file contents across all files in the group.
   - Extends to groups of >2 files by OR-ing all bytes at each position.
   - For each incomplete original file in the group (i.e., that differs from the merged):
     - If --replace mode is enabled:
       - Replace the original file with the merged contents (using a temporary file and rename for safety).
     - Else:
       - Create a new file next to it with the same basename but `.merged` suffix containing the merged contents.
   - For groups with files in different subdirectories, create/replace merged outputs per incomplete file's location (e.g., multiple `.merged` files if needed). This may involve copying merged content across directories for efficiency.
   - Note: Completeness is determined during the merge process by checking if each file's contents match the merged result.
   - Files can be large, so operations are streaming.

6. **Edge Cases**:
   - Single file in a group: Skip, no merge needed.
   - More than two files: Handled by generalizing the sanity check and OR operation across all.
   - Files smaller than 1MB: Ignore.
   - Identical files: Merged result same as original; don't persist .merged.
   - Mismatch in size (though grouped by size): Error.
   - I/O errors: Handle gracefully, log, and continue.
   - Files in different directories with same basename: Each gets its own .merged in its directory, if different from original.

## Implementation Plan

- **Language**: Rust (using standard library for file I/O, no external crates initially for simplicity).
- **Structure**:
  - `main.rs`: CLI parsing, recursive directory scanning, grouping, and orchestration.
  - `merger.rs`: Functions for sanity check and merging.
  - Use `std::fs` and `std::io` for file operations.
  - Implement recursive directory traversal using `std::fs::read_dir`.
  - For efficiency with large files: Read/write in buffered chunks (e.g., 4KB buffers).
  - Use `clap` with derive for CLI, including the --replace flag.
- **Error Handling**: Use `Result` types, log to stderr.
- **Testing**: Unit tests for sanity check and OR logic; integration tests with sample files.
- **Extensions**: Later add progress reporting or handling of more complex scenarios.

## Potential Challenges

- Handling very large files without excessive memory use.
- Ensuring atomicity when writing merged files (e.g., write to temp file then rename).
- Performance: Optimize byte-wise operations for speed.

This design provides a foundation for the application. If requirements change (e.g., handling subdirectories or more than two files), the design can be updated accordingly.
