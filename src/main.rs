use clap::Parser;
use std::path::PathBuf;

#[derive(Parser, Debug)]
#[command(name = "torrent-combine")]
struct Args {
    root_dir: PathBuf,
}

fn main() {
    let args = Args::parse();
    println!("Root dir: {:?}", args.root_dir);
}
