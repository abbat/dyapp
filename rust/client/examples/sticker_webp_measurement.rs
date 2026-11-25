//! Generate representative WebP inputs and expose real first-frame decode measurements.

use dyapp_client::sticker_webp::decode::first_frame;
use image_webp::{ColorType, WebPDecoder, WebPEncoder};
use std::{
    env, fs,
    hint::black_box,
    io::Cursor,
    path::{Path, PathBuf},
    process::ExitCode,
    time::Instant,
};

const SIDE: u32 = 512;
const FRAMES: u32 = 30;
const FRAME_DURATION_MS: u32 = 100;
const MAX_INPUT: usize = 256 * 1024;

struct Metadata {
    width: u32,
    height: u32,
    frames: u32,
    duration_ms: u64,
}

fn pixels(frame: u32) -> Vec<u8> {
    let mut pixels = Vec::with_capacity(SIDE as usize * SIDE as usize * 4);
    for y in 0..SIDE {
        for x in 0..SIDE {
            let marker = x / 64 == frame % 8 && y / 64 == (frame * 3) % 8;
            let rgb = if marker {
                [255, 32, 192]
            } else {
                [
                    (x + frame).to_le_bytes()[0],
                    (y + frame * 2).to_le_bytes()[0],
                    (x + y).to_le_bytes()[0],
                ]
            };
            pixels.extend_from_slice(&rgb);
            pixels.push(255);
        }
    }
    pixels
}

fn encoded(frame: u32) -> Result<Vec<u8>, String> {
    let mut bytes = Vec::new();
    WebPEncoder::new(&mut bytes)
        .encode(&pixels(frame), SIDE, SIDE, ColorType::Rgba8)
        .map_err(|error| error.to_string())?;
    Ok(bytes)
}

fn chunk(kind: [u8; 4], data: &[u8]) -> Result<Vec<u8>, String> {
    let size = u32::try_from(data.len()).map_err(|error| error.to_string())?;
    let mut out = kind.to_vec();
    out.extend_from_slice(&size.to_le_bytes());
    out.extend_from_slice(data);
    if !data.len().is_multiple_of(2) {
        out.push(0);
    }
    Ok(out)
}

fn put_u24(target: &mut [u8], value: u32) {
    target.copy_from_slice(&value.to_le_bytes()[..3]);
}

fn animation() -> Result<Vec<u8>, String> {
    let mut canvas = vec![0x12, 0, 0, 0, 0, 0, 0, 0, 0, 0];
    put_u24(&mut canvas[4..7], SIDE - 1);
    put_u24(&mut canvas[7..10], SIDE - 1);
    let mut body = b"WEBP".to_vec();
    body.extend(chunk(*b"VP8X", &canvas)?);
    body.extend(chunk(*b"ANIM", &[0; 6])?);
    for index in 0..FRAMES {
        let image = encoded(index)?;
        let mut frame = vec![0; 16];
        put_u24(&mut frame[6..9], SIDE - 1);
        put_u24(&mut frame[9..12], SIDE - 1);
        put_u24(&mut frame[12..15], FRAME_DURATION_MS);
        frame[15] = 2;
        frame.extend_from_slice(&image[12..]);
        body.extend(chunk(*b"ANMF", &frame)?);
    }
    let size = u32::try_from(body.len()).map_err(|error| error.to_string())?;
    let mut bytes = b"RIFF".to_vec();
    bytes.extend_from_slice(&size.to_le_bytes());
    bytes.extend(body);
    Ok(bytes)
}

fn metadata(bytes: &[u8]) -> Result<Metadata, String> {
    let decoder = WebPDecoder::new(Cursor::new(bytes)).map_err(|error| error.to_string())?;
    let (width, height) = decoder.dimensions();
    Ok(Metadata {
        width,
        height,
        frames: decoder.num_frames(),
        duration_ms: decoder.loop_duration(),
    })
}

fn validate(bytes: &[u8], frames: u32, duration_ms: u64) -> Result<Metadata, String> {
    if bytes.len() > MAX_INPUT {
        return Err(format!("fixture exceeds {MAX_INPUT} bytes"));
    }
    let frame = first_frame(bytes).map_err(str::to_owned)?;
    let metadata = metadata(bytes)?;
    if (metadata.width, metadata.height) != (SIDE, SIDE)
        || (frame.width, frame.height) != (SIDE, SIDE)
        || metadata.frames != frames
        || metadata.duration_ms != duration_ms
    {
        return Err(format!(
            "fixture metadata {}x{} frames={} duration={}ms, expected frames={} duration={}ms",
            metadata.width,
            metadata.height,
            metadata.frames,
            metadata.duration_ms,
            frames,
            duration_ms,
        ));
    }
    Ok(metadata)
}

fn generate(directory: &Path) -> Result<(), String> {
    fs::create_dir_all(directory).map_err(|error| error.to_string())?;
    let fixtures = [
        ("static-512.webp", encoded(0)?, 0, 0),
        (
            "animated-512-30f-3000ms.webp",
            animation()?,
            FRAMES,
            u64::from(FRAMES * FRAME_DURATION_MS),
        ),
    ];
    for (name, bytes, frames, duration_ms) in fixtures {
        validate(&bytes, frames, duration_ms)?;
        fs::write(directory.join(name), bytes).map_err(|error| error.to_string())?;
    }
    Ok(())
}

fn decode(path: &Path, iterations: usize) -> Result<String, String> {
    let bytes = fs::read(path).map_err(|error| error.to_string())?;
    let metadata = metadata(&bytes)?;
    first_frame(&bytes).map_err(str::to_owned)?;
    let started = Instant::now();
    for _ in 0..iterations {
        black_box(first_frame(black_box(&bytes)).map_err(str::to_owned)?);
    }
    let decode_wall_ns = started.elapsed().as_nanos();
    Ok(format!(
        concat!(
            "{{\"input_bytes\":{},\"width\":{},\"height\":{},\"frames\":{},",
            "\"duration_ms\":{},\"iterations\":{},\"decode_wall_ns\":{}}}"
        ),
        bytes.len(),
        metadata.width,
        metadata.height,
        metadata.frames,
        metadata.duration_ms,
        iterations,
        decode_wall_ns,
    ))
}

fn run() -> Result<Option<String>, String> {
    let args: Vec<String> = env::args().skip(1).collect();
    match args.as_slice() {
        [mode, directory] if mode == "generate" => {
            generate(&PathBuf::from(directory))?;
            Ok(None)
        }
        [mode, path] if mode == "baseline" => {
            black_box(fs::read(path).map_err(|error| error.to_string())?);
            Ok(None)
        }
        [mode, path, iterations] if mode == "decode" => {
            let iterations = iterations
                .parse::<usize>()
                .ok()
                .filter(|value| (1..=10_000).contains(value))
                .ok_or("iterations must be in 1..=10000")?;
            decode(&PathBuf::from(path), iterations).map(Some)
        }
        _ => Err("usage: generate DIR | baseline FILE | decode FILE ITERATIONS".into()),
    }
}

fn main() -> ExitCode {
    match run() {
        Ok(output) => {
            if let Some(output) = output {
                println!("{output}");
            }
            ExitCode::SUCCESS
        }
        Err(error) => {
            eprintln!("{error}");
            ExitCode::FAILURE
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn representative_fixtures_are_bounded_and_nonuniform() {
        for (bytes, frames, duration) in [
            (encoded(0).unwrap(), 0, 0),
            (animation().unwrap(), FRAMES, 3_000),
        ] {
            validate(&bytes, frames, duration).unwrap();
            let decoded = first_frame(&bytes).unwrap();
            let first: [u8; 4] = decoded.pixels[..4].try_into().unwrap();
            assert!(decoded
                .pixels
                .as_chunks::<4>()
                .0
                .iter()
                .any(|pixel| pixel != &first));
        }
    }
}
