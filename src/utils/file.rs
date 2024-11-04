use std::fs::File;
use std::io::prelude::*;
pub fn save_file(filename: &str, filedata: Vec<u8>) -> std::io::Result<()> {
    let mut file = File::create(format!("./public/{}", filename))?;
    file.write_all(&filedata)?;
    Ok(())
}
