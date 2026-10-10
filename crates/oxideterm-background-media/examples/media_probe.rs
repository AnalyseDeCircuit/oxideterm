//! Read a background stream without a GUI to inspect its decoded pixels and media timing.

use oxideterm_background_media::{
    BackgroundFit, MediaReply, MediaStream, OutputParams, PlaybackLimits,
};
use std::{path::PathBuf, time::Duration};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut args = std::env::args_os().skip(1);
    let path = args
        .next()
        .map(PathBuf::from)
        .ok_or("usage: media_probe FILE [WIDTH HEIGHT FRAMES]")?;
    let mut parameters = [8192, 8192, 56];
    for parameter in &mut parameters {
        if let Some(value) = args.next() {
            *parameter = value.to_str().ok_or("invalid numeric argument")?.parse()?;
        }
    }
    let mut stream = MediaStream::spawn(path, 1)?;
    let output = OutputParams {
        width: parameters[0],
        height: parameters[1],
        fit: BackgroundFit::Contain,
        blur: 0.0,
        limits: PlaybackLimits::default(),
    };
    for _ in 0..parameters[2] {
        if !stream.request_next(Duration::ZERO, output) {
            return Err("request was not accepted".into());
        }
        match futures::executor::block_on(stream.recv()).ok_or("decoder closed")? {
            MediaReply::Frame { frame, .. } => println!(
                "{} {} {} {} {} {} {} {} {}",
                frame.sequence,
                frame.timestamp.as_millis(),
                frame.duration.as_millis(),
                frame.width,
                frame.height,
                frame.pixels[0],
                frame.pixels[1],
                frame.pixels[2],
                frame.pixels[3]
            ),
            MediaReply::End => break,
            MediaReply::Error(error) => return Err(error.into()),
        }
    }
    Ok(())
}
