# Torrent Combine

A Rust CLI tool to merge partially downloaded torrent files (e.g., videos) within a directory tree. It groups files by name and size, performs sanity checks for compatibility, and merges them using bitwise OR on their contents. Merged files are saved with a `.merged` suffix or can replace originals with the `--replace` flag.

## Description

This tool scans a root directory recursively for files larger than 1MB (targeting video files). It assumes partial torrent downloads are pre-allocated with zeros and merges compatible files:

- **Grouping**: Files with identical basenames and sizes.
- **Sanity Check**: Non-zero bytes at each position must match across files.
- **Merge**: Bitwise OR of contents to combine downloaded chunks.
- **Output**: Creates `.merged` files for incomplete originals (unless `--replace` is used to overwrite them).
- Skips groups if all files are already complete or if sanity fails.

For details, see [DESIGN.md](DESIGN.md).

## Installation

Requires Rust and Cargo (install via [rustup](https://rustup.rs/)).

Clone the repository:

```bash
git clone <repo-url>
cd torrent-combine
```

Build the project:

```bash
cargo build --release
```

The binary will be in `target/release/torrent-combine`.

## Usage

Run the tool with a root directory path:

```bash
cargo run -- /path/to/root/dir
```

Or with the built binary:

```bash
./target/release/torrent-combine /path/to/root/dir
```

### Options

- `--replace`: Replace incomplete original files with merged content instead of creating `.merged` files.

Enable debug logging:

```bash
RUST_LOG=debug cargo run -- /path/to/root/dir
```

## Examples

Assume two partial files `/downloads/video.mkv` (size 10MB, partial) and `/other/video.mkv` (size 10MB, more complete):

```bash
cargo run -- /downloads
```

This creates `/downloads/video.mkv.merged` if the original is incomplete.

With replace:

```bash
cargo run -- /downloads --replace
```

This overwrites the incomplete `/downloads/video.mkv` with the merged content.

## Contributing

Fork the repo, make changes, and submit a pull request. See [CONVENTIONS.md](CONVENTIONS.md) for coding standards.

## License

MIT License (or specify your license).
