use clap::Parser;
use gstreamer as gst;
use producer::{FrameCounter, GstProducer, Producer, ProducerMessage};
use render::{DefaultRenderer, KittyRenderer, Renderer};
use std::error::Error;
use std::io::Write;
use std::ops::{Deref, DerefMut};
use std::sync::atomic::AtomicBool;
use std::sync::mpsc::Receiver;

use std::{
    io::{stdout, BufWriter},
    path::PathBuf,
    time::Duration,
};
mod buffer;
mod color;
mod producer;
mod render;
mod resize_watcher;

use color::{Ansi256, BackgroundAnsi256, BackgroundRgb, Rgb};
use crate::resize_watcher::ResizeWatcher;

/// Play a video in the terminal from a file path or url.
#[derive(Parser)]
pub struct Args {
    /// The file or url to play
    video: String,
    /// Interpret the video as a file or url
    #[arg(short, long, default_value_t = false)]
    url: bool,
    /// Use 256 colors instead of truecolor. This may speed up the rendering at the cost of color quality.
    #[arg(short, long, default_value_t = false)]
    ansi256: bool,
    /// The maximum amount of time to wait for the decoder to get the source capabilities
    #[arg(short, long, default_value_t = 5)]
    timeout: u64,

    /// (Experimental and buggy) Use the kitty image protocol.
    #[arg(short, long, default_value_t = false)]
    kitty: bool,
    /// Use the colors as the background of the pixel instead of the foreground. This is the recommended mode and may become default in the future.
    #[arg(short, long, default_value_t = false)]
    background: bool,
}

fn hide_cursor(mut out: impl Write) -> std::io::Result<()> {
    out.write_all(b"\x1b[?25l")?;
    Ok(())
}
fn show_cursor(mut out: impl Write) -> std::io::Result<()> {
    out.write_all(b"\x1b[?25h")?;
    Ok(())
}

/// A wrapper around a `Write` that hides the cursor on creation and shows it again on drop
pub struct HideCursor<W: Write>(W);
impl<W: Write> HideCursor<W> {
    pub fn new(mut writer: W) -> Self {
        let _ = hide_cursor(&mut writer);
        Self(writer)
    }
    pub fn show(&mut self) -> std::io::Result<()> {
        show_cursor(&mut self.0)
    }
}
impl<W: Write> Drop for HideCursor<W> {
    fn drop(&mut self) {
        let _ = self.show();
    }
}
impl<W: Write> Deref for HideCursor<W> {
    type Target = W;
    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl<W: Write> DerefMut for HideCursor<W> {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.0
    }
}

fn print_dropped_frames(counter: &FrameCounter, mut write: impl Write) {
    write!(write, "\n\n\n\x1b[0m{counter}").unwrap();
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args = Args::parse();
    let file = if args.url {
        args.video
    } else {
        // gstreamer expects a url like this
        format!(
            "file://{}",
            PathBuf::from(args.video).canonicalize()?.display()
        )
    };

    let termsize = termsize::get().unwrap();
    let (termwidth, termheight) = (termsize.cols, termsize.rows);

    let out = BufWriter::with_capacity(
        (termwidth as usize * termheight as usize) * 18, // have room for slightly above the worst case where we need an escape sequence for each pixel on the screen
        stdout().lock(),
    );
    let mut out = HideCursor::new(out);

    gst::init()?;

    // Resize with half the height because the terminal font is generally ~1:2 aspect ratio.
    // Use rgbx format because we will use the `x` to store the character printed.
    // Except kitty just wants either rgb or rgba, so we will opt into the latter
    let (params, format) = if !args.kitty {
        (
            format!("width={termwidth},height={termheight},pixel-aspect-ratio=1/2"),
            "RGBx",
        )
    } else {
        ("pixel-aspect-ratio=1/1".to_owned(), "RGBA")
    };
    let mut producer = producer::GstProducer::new(
        &format!(
            "playbin uri=\"{file}\" video-sink=\"videoconvert
        ! videoscale 
        ! capsfilter name=caps caps=video/x-raw,{params},format={format}
        ! appsink name=app_sink
        ! sink_to_location\"",
        ),
        Duration::from_secs(args.timeout),
    )?;

    let wait = &producer.subscribe();
    let o = &mut *out;
    match (args.kitty, args.ansi256, args.background) {
        // kitty
        (true, _, _) => {
            o.write_all(b"\x1b[0;0H")?;
            do_run::<KittyRenderer>(wait, &producer, o)?;
        }
        // ansi + background
        (_, true, true) => do_run::<DefaultRenderer<BackgroundAnsi256>>(wait, &producer, o)?,
        // ansi + not background
        (_, true, false) => do_run::<DefaultRenderer<Ansi256>>(wait, &producer, o)?,
        // rgb + background
        (_, false, true) => do_run::<DefaultRenderer<BackgroundRgb>>(wait, &producer, o)?,
        // rgb + not background
        (_, false, false) => do_run::<DefaultRenderer<Rgb>>(wait, &producer, o)?,
    }

    print_dropped_frames(&producer.counter(), &mut *out);
    Ok(())
}

fn do_run<R: Renderer>(
    wait: &Receiver<ProducerMessage>,
    producer: &GstProducer,
    mut out: impl Write,
) -> Result<(), Box<dyn Error>>
where
{
    let mut renderer = None;
    let mut state = None;
    let interrupt = std::sync::Arc::new(AtomicBool::new(false));
    let i = interrupt.clone();
    ctrlc::set_handler(move || i.store(true, std::sync::atomic::Ordering::Relaxed))
        .expect("failed to set interrupt handler");

    let mut resize_watcher = resize_watcher::default_watcher()
        .expect("failed to listen for terminal resizes");

    while let Ok(msg) = wait.recv_timeout(Duration::from_secs(3)) {
        if interrupt.load(std::sync::atomic::Ordering::Relaxed) {
            break;
        }
        match msg {
            ProducerMessage::Initialize { width, height } => {
                let r = R::from_dims(width, height);

                state = Some(r.create_state());
                renderer = Some(r);

                write!(out, "\x1b[2J")?; // clear the screen
            }
            ProducerMessage::FrameReady => {
                let r = renderer.as_mut().expect("renderer should be initialized");
                let state = state.as_mut().expect("differ should be initialized");
                {
                    let frame = producer.frame().expect("frame should be ready");
                    let frame = r.verify_input(&frame);
                    r.consume(frame);
                }
                r.render_frame(&mut out, state)?;
            }
        }

        if resize_watcher.resized() {
            // Tell producer to change the size of new video frames
            // The renderer can't be resized yet, since there may still be unrendered frames that use the previous resolution
            let termsize = termsize::get().unwrap();
            let (termwidth, termheight) = (termsize.cols, termsize.rows);
            producer.resize(termwidth as u32, termheight as u32);
        }
    }
    Ok(())
}
