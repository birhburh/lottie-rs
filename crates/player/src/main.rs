#![feature(path_file_prefix)]
use std::fs;
use std::io::Write;
use std::path::Path;

// use bevy_prototype_debug_lines::{DebugLines, DebugLinesPlugin};
use anyhow::Error;
use clap::Parser;
use lottie_core::{Config, HeadlessConfig, Lottie, Renderer, Target, WindowConfig};
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
    /// Show controls, this options is invalid if `headless` is enabled
    #[clap(long, action)]
    controls: bool,
    /// Show EGUI inspector for debugging, this options is invalid if `headless`
    /// is enabled
    #[clap(long, action)]
    inspector: bool,
}

// fn axis_system(mut lines: ResMut<DebugLines>) {
//     lines.line(Vec3::new(0.0, 250.0, 0.0), Vec3::new(0.0, -250.0, 0.0), 1.0);
//     lines.line(Vec3::new(250.0, 0.0, 0.0), Vec3::new(-250.0, 0.0, 0.0), 1.0);
// }

fn main() -> Result<(), Error> {
    let args = Args::parse();
    let path = Path::new(&args.input);
    let mut root_path = path.to_path_buf();
    root_path.pop();
    let mut filename = path
        .file_prefix()
        .and_then(|name| name.to_str())
        .unwrap()
        .to_string();
    if filename.is_empty() {
        filename = "output".to_string();
    }
    let root_path = &*root_path.to_string_lossy();
    let f = fs::File::open(path).unwrap();
    let lottie = Lottie::from_reader(f, root_path).unwrap();
    let final_timestamp = (lottie.model.end_frame / lottie.model.frame_rate * 1000.0) as i32;
    let mut encoder = Encoder::new((lottie.model.width, lottie.model.height))?;
    let (mut renderer, frame_stream) = BevyRenderer::new();
    let config = if args.headless {
        Config::Headless(HeadlessConfig {
            target: Target::Default,
            filename,
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
    smol::block_on::<Result<_, Error>>(async {
        // renderer.add_plugin(DebugLinesPlugin::default());
        // renderer.add_system(axis_system);
        renderer.load_lottie(lottie, config);
        renderer.render();
        pin!(frame_stream);
        while let Some(frame) = frame_stream.next().await {
            encoder.add_frame(&frame.data, frame.timestamp)?;
        }
        Ok(())
    })?;
    let data = encoder.finalize(final_timestamp)?;
    if let Some(filename) = filename {
        let mut f = std::fs::File::create(&format!("{filename}.webp"))?;
        f.write_all(&data)?;
        drop(f);
    }
    Ok(())
}
