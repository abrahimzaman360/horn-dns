use clap::Parser;
use serde::{Deserialize, Serialize};
use std::{
    fs::{self, File},
    io::Write,
    path::Path,
};

#[derive(Parser, Debug)]
#[command(version, about)]
struct Args {
    #[arg(short)]
    name: Option<String>,
}

fn save(filename: &str) -> Result<File, std::io::Error> {
    let dir = Path::new("./data");
    fs::create_dir(dir)?;
    let path = dir.join(filename);
    let mut file = File::create(path)?;
    file.write_all("Beautiful You!".as_bytes())?;

    Ok(file)
}

fn main() {
    let args = Args::parse();
    let x: Never = !;

    if let Some(arg) = args.name {
        let f = save(&arg);
        match f {
            Ok(a) => {
                dbg!(a);
            }
            Err(e) => {
                dbg!(e);
            }
        }
    }
}
