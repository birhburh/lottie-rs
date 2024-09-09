use std::ffi::OsStr;
// #![feature(path_file_prefix)]
use std::fs::{self, File};
use std::io::{BufWriter, Write};
use std::path::Path;

// use bevy_prototype_debug_lines::{DebugLines, DebugLinesPlugin};
use anyhow::Error;
use clap::Parser;
use lottie::{Config, HeadlessConfig, Lottie, Renderer, Target, WindowConfig};
use lottie_renderer_bevy::BevyRenderer;
use smol::pin;
use smol::stream::StreamExt;
use webp_animation::Encoder;

#[derive(Parser)]
#[clap(author, version, about, long_about = None)]
struct Args {
    /// Input file, should be a Lottie JSON
    #[clap(short, long)]
    input: String,
    /// Run in headless mode, a animation file with the same name as the input
    /// will be generated
    #[clap(long, action)]
    headless: bool,
    #[clap(long)]
    frames: bool,
    #[clap(long)]
    frame: Option<u32>,
    /// Show controls, this options is invalid if `headless` is enabled
    #[clap(long, action)]
    controls: bool,
    /// Show EGUI inspector for debugging, this options is invalid if `headless`
    /// is enabled
    #[clap(long, action)]
    inspector: bool,
    #[clap(long)]
    scale: Option<f32>,
}

// fn axis_system(mut lines: ResMut<DebugLines>) {
//     lines.line(Vec3::new(0.0, 250.0, 0.0), Vec3::new(0.0, -250.0, 0.0), 1.0);
//     lines.line(Vec3::new(250.0, 0.0, 0.0), Vec3::new(-250.0, 0.0, 0.0), 1.0);
// }

fn os_str_as_u8_slice(s: &OsStr) -> &[u8] {
    unsafe { &*(s as *const OsStr as *const [u8]) }
}
unsafe fn u8_slice_as_os_str(s: &[u8]) -> &OsStr {
    // SAFETY: see the comment of `os_str_as_u8_slice`
    unsafe { &*(s as *const [u8] as *const OsStr) }
}

fn split_file_at_dot(file: &OsStr) -> (&OsStr, Option<&OsStr>) {
    let slice = os_str_as_u8_slice(file);
    if slice == b".." {
        return (file, None);
    }

    // The unsafety here stems from converting between &OsStr and &[u8]
    // and back. This is safe to do because (1) we only look at ASCII
    // contents of the encoding and (2) new &OsStr values are produced
    // only from ASCII-bounded slices of existing &OsStr values.
    let i = match slice[1..].iter().position(|b| *b == b'.') {
        Some(i) => i + 1,
        None => return (file, None),
    };
    let before = &slice[..i];
    let after = &slice[i + 1..];
    unsafe { (u8_slice_as_os_str(before), Some(u8_slice_as_os_str(after))) }
}

pub fn file_prefix(path: &Path) -> Option<&OsStr> {
    path.file_name().map(split_file_at_dot).and_then(|(before, _after)| Some(before))
}

fn main() -> Result<(), Error> {
    let args = Args::parse();
    let path = Path::new(&args.input);
    let mut root_path = path.to_path_buf();
    root_path.pop();
    let mut filename =
        file_prefix(path)
        .and_then(|name| name.to_str())
        .unwrap()
        .to_string();
    if filename.is_empty() {
        filename = "output".to_string();
    }
    let root_path = &*root_path.to_string_lossy();
    let f = fs::File::open(path).unwrap();
    let mut lottie = Lottie::from_reader(f, root_path).unwrap();
    lottie.scale = args.scale.unwrap_or(1.0);
    let final_timestamp = (lottie.model.end_frame / lottie.model.frame_rate * 1000.0) as i32;
    let (mut renderer, frame_stream) = BevyRenderer::new();
    let config = if args.headless {
        Config::Headless(HeadlessConfig {
            target: Target::Default,
            filename,
            frame: args.frame,
        })
    } else {
        Config::Window(WindowConfig {
            show_controls: args.controls,
            show_inspector: args.inspector,
        })
    };
    let filename = if let Config::Headless(HeadlessConfig { filename, .. }) = &config {
        Some(filename.clone())
    } else {
        None
    };
    let mut target_frame = if let Config::Headless(HeadlessConfig { frame, .. }) = &config {
        frame.clone()
    } else {
        None
    };
    let all_frames = args.frames;

    let width = (lottie.model.width as f32 * lottie.scale).round() as u32;
    let height = (lottie.model.height as f32 * lottie.scale).round() as u32;
    let mut size = (width, height);
    let mut encoder = Encoder::new(size)?;
    smol::block_on::<Result<_, Error>>(async {
        // renderer.add_plugin(DebugLinesPlugin::default());
        // renderer.add_system(axis_system);
        renderer.load_lottie(lottie, config);
        renderer.render();
        pin!(frame_stream);
        let mut i = 0;
        if all_frames {
            target_frame = Some(0);
        }
        while let Some(frame) = frame_stream.next().await {
            if let (Some(target), Some(filename)) = (target_frame, filename.as_ref()) {
                if target == i {
                    let f = File::create(&format!("{}_{}.png", filename, i))?;
                    let w = BufWriter::new(f);
                    let mut encoder = png::Encoder::new(w, frame.width, frame.height);
                    encoder.set_color(png::ColorType::Rgba);
                    encoder.set_depth(png::BitDepth::Eight);
                    encoder
                        .write_header()
                        .unwrap()
                        .write_image_data(&frame.data)
                        .unwrap();
                    if !all_frames {
                        break;
                    }
                }
                i += 1;
                if all_frames {
                    target_frame = Some(i);
                }
            } else {
                if size.0 != frame.width || size.1 != frame.height {
                    size = (frame.width, frame.height);
                    encoder = Encoder::new(size)?;
                }
                encoder.add_frame(&frame.data, frame.timestamp)?;
            }
        }
        Ok(())
    })?;
    if target_frame.is_none() {
        let data = encoder.finalize(final_timestamp)?;
        if let Some(filename) = filename {
            let mut f = std::fs::File::create(&format!("{filename}.webp"))?;
            f.write_all(&data)?;
            drop(f);
        }
    }
    Ok(())
}
