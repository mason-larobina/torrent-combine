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

Both require Rust and Cargo (install via [rustup](https://rustup.rs/)).

### Via cargo install

```bash
cargo install torrent-combine
```

### From source

```bash
git clone https://github.com/mason-larobina/torrent-combine
cd torrent-combine
cargo install --path=.
```

## Usage

Run the tool with a root directory path:

```bash
torrent-combine /path/to/torrent/root/dir
```

### Options

- `--replace`: Replace incomplete original files with merged content instead of creating `.merged` files.

## Examples

Assume two partial files `/downloads/torrent-a/video.mkv` (size 10MB, partial) and `/downloads/torrent-b/video.mkv` (size 10MB, more complete):

```bash
torrent-combine /downloads
```

This creates `/downloads/torrent-a/video.mkv.merged` if the `torrent-a/video.mkv` was able to fill in missing chunks from `torrent-b/video.mkv`.

Likewise the `/downloads/torrent-b/video.mkv.merged` file is created if the `torrent-b/video.mkv` file was able to fill in missing chunks from `torrent-a/video.mkv`.

To merge the files in-place use the replace flag:

```bash
torrent-combine /downloads --replace
```

This overwrites the incomplete `/downloads/torrent-a/video.mkv` and or `/downloads/torrent-b/video.mkv` with the merged content if applicable.

## Contributing

Fork the repo, make changes, and submit a pull request. See [CONVENTIONS.md](CONVENTIONS.md) for coding standards.

## License

MIT License.
