use mp3lame_encoder::{Builder, FlushNoGap, MonoPcm};
use std::fs::File;
use std::io::prelude::*;
pub fn save_file(filename: &str, filedata: Vec<u8>) -> std::io::Result<()> {
    let mut file = File::create(format!("./public/{}", filename))?;
    file.write_all(&filedata)?;
    Ok(())
}

pub fn save_audio_file(filename: &str, filedata: Vec<i16>) -> Result<(), String> {
    let mut mp3_encoder = Builder::new().expect("Create LAME builder");
    mp3_encoder.set_num_channels(2).expect("set channels");
    mp3_encoder
        .set_sample_rate(44_100)
        .map_err(|e| e.to_string())?;
    mp3_encoder
        .set_brate(mp3lame_encoder::Bitrate::Kbps192)
        .map_err(|e| e.to_string())?;
    mp3_encoder
        .set_quality(mp3lame_encoder::Quality::Best)
        .map_err(|e| e.to_string())?;
    let mut mp3_encoder = mp3_encoder.build().expect("To initialize LAME encoder");
    let input = MonoPcm(filedata.as_slice());

    let mut mp3_out_buffer = Vec::new();
    mp3_out_buffer.reserve(mp3lame_encoder::max_required_buffer_size(filedata.len()));
    let encoded_size = mp3_encoder
        .encode(input, mp3_out_buffer.spare_capacity_mut())
        .map_err(|e| e.to_string())?;
    unsafe {
        mp3_out_buffer.set_len(mp3_out_buffer.len().wrapping_add(encoded_size));
    }

    let encoded_size = mp3_encoder
        .flush::<FlushNoGap>(mp3_out_buffer.spare_capacity_mut())
        .map_err(|e| e.to_string())?;
    unsafe {
        mp3_out_buffer.set_len(mp3_out_buffer.len().wrapping_add(encoded_size));
    }
    let mut file = File::create(format!("./public/{}", filename)).map_err(|e| e.to_string())?;
    file.write_all(&mp3_out_buffer).map_err(|e| e.to_string())?;
    return Ok(());
}
