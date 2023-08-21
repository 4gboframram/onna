use clap::Parser;
use gst::prelude::*;
use gstreamer as gst;
use gstreamer_app as gst_app;
use std::cmp::Ordering;
use std::error::Error;
use std::io::Write;
use std::ops::{Deref, DerefMut};
use std::str::from_utf8;
use std::sync::mpsc::{self, Receiver};
use std::sync::{Arc, Mutex};
use std::{
    io::{stdout, BufWriter},
    ops::Range,
    path::PathBuf,
    slice::from_raw_parts,
    sync::atomic::AtomicUsize,
    time::Duration,
};

// original 70 character gradient
// const ASCII_CHARS: &str =
//     "$@B%8&WM#*oahkbdpqwmZO0QLCJUYXzcvunxrjft/\\|()1{}[]?-_+~<>i!lI;:,\"^`'. ";

// reversed for light mode
// const ASCII_CHARS: &str = "$@B%8&W#*oahkbdpqwmZOQCJUYXzcvuxrjft/\\|()1{}[]?-_+~<>i!lI;:,\"`. ";
const ASCII_CHARS: &str = " .`\",:;Il!i><~+_-?][}{1)(|\\/tfjrxuvczXYUJCQOZmwqpdbkhao*#W&8%B@$";

pub struct BufferDiffIter<'a, T: PartialEq + Clone> {
    current: &'a [T],
    prev: &'a [T],
    idx: usize,
}

impl<'a, T: PartialEq + Clone> BufferDiffIter<'a, T> {
    pub fn new(current: &'a [T], prev: &'a [T]) -> Self {
        assert_eq!(
            prev.len(),
            current.len(),
            "both current and prev must be the same length"
        );
        Self {
            current,
            prev,
            idx: 0,
        }
    }
}

impl<'a, T: PartialEq + Clone> Iterator for BufferDiffIter<'a, T> {
    type Item = (Range<usize>, T);
    fn next(&mut self) -> Option<Self::Item> {
        while self.prev.get(self.idx)? == self.current.get(self.idx)? {
            self.idx += 1;
        }
        let start = self.idx;
        let item = self.current.get(self.idx)?;
        loop {
            match self.current.get(self.idx) {
                Some(i) if i == item && i != &self.prev[self.idx] => self.idx += 1,
                _ => return Some((start..self.idx, item.clone())),
            }
        }
    }
}

pub trait Colorize: PartialEq + Default {
    fn from_rgb(rgb: [u8; 3]) -> Self;
    fn write_escape(&self, out: &mut impl Write) -> std::io::Result<()>;
}

#[derive(Clone, Copy, PartialEq, Default)]
pub struct RGB([u8; 3]);

impl Colorize for RGB {
    fn from_rgb(rgb: [u8; 3]) -> Self {
        Self(rgb)
    }
    fn write_escape(&self, out: &mut impl Write) -> std::io::Result<()> {
        let Self([r, g, b]) = *self;
        write!(out, "\x1b[38;2;{r};{g};{b}m")
    }
}

#[derive(Clone, Copy, PartialEq, Default)]
pub struct XTerm256(u8);

impl Colorize for XTerm256 {
    fn from_rgb(rgb: [u8; 3]) -> Self {
        Self(ansi_colours::ansi256_from_rgb(rgb))
    }
    fn write_escape(&self, out: &mut impl Write) -> std::io::Result<()> {
        let ansi = self.0;
        write!(out, "\x1b[38;5;{ansi}m")
    }
}

const fn luminance(pixel: [u8; 4]) -> u8 {
    let [r, g, b, _] = pixel;
    (((r as u32) * 3 + (b as u32) + ((g as u32) << 2)) >> 3) as u8
}

fn normalize_luminance(pixel: [u8; 4], luminance: u8) -> [u8; 4] {
    let [r, g, b, ch] = pixel;
    let inv_lum = 1u32 << 8 / (luminance as u32 + 1);
    let r = (r as u32) << 8;
    let g = (g as u32) << 8;
    let b = (b as u32) << 8;

    let r = ((r * inv_lum) >> 8).min(u8::MAX as u32) as u8;
    let g = ((g * inv_lum) >> 8).min(u8::MAX as u32) as u8;
    let b = ((b * inv_lum) >> 8).min(u8::MAX as u32) as u8;

    [r, g, b, ch]
}
pub struct Differ<C: Colorize> {
    data: Vec<(Range<usize>, C, u8)>,
}

impl<C: Colorize> Differ<C> {
    pub fn new(width: u32, height: u32) -> Self {
        Self {
            data: Vec::with_capacity(width as usize * height as usize),
        }
    }
    pub fn assign_diff(&mut self, curr: &[[u8; 4]], prev: &[[u8; 4]]) {
        self.data.clear();
        let diff_iter = BufferDiffIter::new(curr, prev)
            .map(|(pos, [r, g, b, chr])| (pos, C::from_rgb([r, g, b]), chr));

        self.data.extend(diff_iter);

        // Comparing the both the colors and the character seems to produce much less dropped frames on my machine
        // than just comparing color, even though it is more likely to produce different colors next to each other.
        // My hypothesis is that it's because it does less swaps.

        // I've literally tried every single combination of `Ordering`, stableness, and comparing 1 vs 2 elements
        // and found through benchmarking that this was the fastest of them all.
        //
        // `sort_by` usually produced between 1-1.5 orders of magnitude more dropped frames compared to `sort_unstable_by`
        self.data.sort_unstable_by(|(_, x, c1), (_, y, c2)| {
            if (x, c1) == (y, c2) {
                Ordering::Less
            } else {
                Ordering::Greater
            }
        });
    }
    pub fn data(&self) -> &[(Range<usize>, C, u8)] {
        &self.data
    }
}
pub struct Renderer {
    width: u32,
    height: u32,
    term_height: u32,

    // [r, g, b, char]
    color_buf: Box<[[u8; 4]]>,
    prev_buf: Box<[[u8; 4]]>,
}

impl Renderer {
    pub fn new(width: u32, height: u32, term_height: u32) -> Self {
        let num_pixels = width * height;
        let color_buf = vec![[0u8, 0, 0, 0]; num_pixels as usize].into_boxed_slice();

        Self {
            width,
            height,
            term_height,

            prev_buf: color_buf.clone(),
            color_buf,
        }
    }
    pub fn process_frame(&mut self, data: &[u8]) {
        assert_eq!(data.len() as u32, self.width * self.height * 4);
        let ptr = data.as_ptr().cast::<[u8; 4]>();
        let data = unsafe { from_raw_parts(ptr, (self.width * self.height) as _) };
        std::mem::swap(&mut self.color_buf, &mut self.prev_buf);
        for (i, pixel) in data.iter().enumerate() {
            let lum = luminance(*pixel);
            let index = lum >> 2;
            let mut pixel = pixel.clone();
            pixel[3] = ASCII_CHARS.as_bytes()[index as usize];
            self.color_buf[i] = normalize_luminance(pixel, lum);
        }
    }
    pub fn render_meow(&self, data: &mut [u8], mut out: impl Write) -> std::io::Result<()> {
        assert_eq!(data.len() as u32, self.width * self.height * 4);
        use base64ct::{Base64, Encoding};

        let data = Base64::encode_string(data);
        let mut iter = data.as_bytes().chunks(4096).peekable();
        while let Some(chunk) = iter.next() {
            let m = iter.peek().is_some() as u8;
            // let s = Base64::encode_string(chunk);
            let s = from_utf8(chunk).unwrap();
            write!(
                out,
                "\x1b_Ga=T,f=32,s={},v={},C=1,m={},x=1,y=1;{s}\x1b\\",
                self.width, self.height, m
            )?;
        }
        out.flush()
    }
    pub fn create_differ<C: Colorize>(&self) -> Differ<C> {
        Differ::new(self.width, self.height)
    }
    pub fn render_frame<C: Colorize + Clone>(
        &self,
        output: &mut impl Write,
        differ: &mut Differ<C>,
    ) -> std::io::Result<()> {
        // not implemented yet
        // // if there's a lot changed between frames  (more than ~50% of the total area, in stride count)
        // // then just rerender the entire screen
        // if difs.len() >= ((self.width * (self.height)) as usize >> 1) {
        //     return self.render_full::<C>(output);
        // }

        // profiling suggests that we are almost 100% io-bound, so we are basically free to do any optimization on escape sequences
        differ.assign_diff(&self.color_buf, &self.prev_buf);

        let mut prev_end: usize = 0;
        let mut prev_color = C::default();

        for (pos, color, chr) in differ.data() {
            // let color = C::from_rgb([r, g, b]);

            // If the previous end is the same as the start, that means the cursor is in the right position
            // and therefore we do not need to print the escape to skip to the line,
            // unless the requred position *is* the origin.
            // In that case, we almost always need to jump to it.
            if pos.start != prev_end || prev_end == 0 {
                let line = pos.start / self.width as usize;
                let column = pos.start % self.width as usize;
                // it is almost always less characters to skip directly to the line and column than to use relative motion
                // maybe i'll optimize that too
                write!(output, "\x1b[{};{}H", line, column)?;
            }
            if color != &prev_color {
                color.write_escape(output)?;
            }

            for i in pos.clone() {
                let col = i % self.width as usize;
                if col == 0 {
                    output.write(b"\n")?;
                    color.write_escape(output)?;
                }
                output.write(&[*chr])?;
            }
            prev_end = pos.end;
            prev_color = color.clone();
        }

        output.flush()?;
        Ok(())
    }
}

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
    /// This may or may not cause the terminal to freeze upon program exit.
    #[arg(short, long, default_value_t = false)]
    kitty: bool,
}

fn hide_cursor(mut out: impl Write) -> std::io::Result<()> {
    out.write(b"\x1b[?25l")?;
    Ok(())
}
fn show_cursor(mut out: impl Write) -> std::io::Result<()> {
    out.write(b"\x1b[?25h")?;
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

fn print_dropped_frames(
    dropped_counter: Arc<AtomicUsize>,
    frame_counter: Arc<AtomicUsize>,
    mut write: impl Write,
) {
    let dropped = dropped_counter.load(std::sync::atomic::Ordering::Relaxed);
    let not_dropped = frame_counter.load(std::sync::atomic::Ordering::Relaxed);
    let total = not_dropped + dropped;
    write!(
        write,
        "\n\n\n\x1b[0mdropped {dropped} frames of {total} ({:.2}%)",
        dropped as f32 / total as f32 * 100.
    )
    .unwrap();
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args = Args::parse();
    let file = if args.url {
        args.video
    } else {
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

    write!(out, "\x1b[2J")?; // clear the screen
    gst::init()?;

    // resize with half the height because the terminal font is generally ~1:2 aspect ratio
    // use rgbx format because we will use the `x` to store the character printed
    //
    let (params, format) = if !args.kitty {
        (
            format!("width={termwidth},height={termheight},pixel-aspect-ratio=1/2"),
            "RGBx",
        )
    } else {
        (
            format!(
                "pixel-aspect-ratio=1/1,width={},height={}",
                termwidth * 4,
                termheight * 4 // arbitrary size. i don't feel like getting the terminal window size currently.
                               // kitty's protocol kinda sucks tbh
                               // and apparently frame skipping doesn't work either for some reason
            ),
            "RGBA",
        )
    };
    let source = gst::parse_launch(&format!(
        "playbin uri=\"{}\" video-sink=\"videoconvert
        ! videoscale 
        ! appsink name=app_sink caps=video/x-raw,{params},format={format}
        ! sink_to_location\"",
        file
    ))?;

    let source = source.downcast::<gst::Bin>().unwrap();

    let video_sink: gst::Element = source.property("video-sink").unwrap().get().unwrap();
    let pad = video_sink.pads().get(0).cloned().unwrap();
    let pad = pad.dynamic_cast::<gst::GhostPad>().unwrap();
    let bin = pad
        .parent_element()
        .unwrap()
        .downcast::<gst::Bin>()
        .unwrap();

    let app_sink = bin.by_name("app_sink").unwrap();
    let app_sink = app_sink.downcast::<gst_app::AppSink>().unwrap();

    let (notify, wait) = mpsc::sync_channel(1);

    let renderer = Arc::new(Mutex::new(Renderer::new(0, 0, termheight as _)));
    let renderer_clone = renderer.clone();

    let frame_ref = Arc::new(Mutex::new(Vec::new()));
    let frame_ref_clone = frame_ref.clone();

    let dropped_frames_counter = Arc::new(AtomicUsize::new(0)); // used by the callback
    let dropped_frames_counter_2 = dropped_frames_counter.clone(); // used on success
    let dropped_frames_counter_3 = dropped_frames_counter.clone(); // used on ctrlc

    let frame_counter = Arc::new(AtomicUsize::new(0)); // used by the callback
    let frame_counter_2 = frame_counter.clone(); // used on success
    let frame_counter_3 = frame_counter.clone(); // used on ctrlc

    let mut check_width = true;

    ctrlc::set_handler(move || {
        let _ = show_cursor(std::io::stderr());
        print_dropped_frames(
            dropped_frames_counter_3.clone(),
            frame_counter_3.clone(),
            std::io::stderr(),
        );
        std::process::exit(1);
    })?;

    app_sink.set_callbacks(
        gst_app::AppSinkCallbacks::builder()
            .new_sample(move |sink| {
                let sample = sink.pull_sample().map_err(|_| gst::FlowError::Eos)?;
                let buffer = sample.buffer().ok_or(gst::FlowError::Error)?;
                let map = buffer.map_readable().map_err(|_| gst::FlowError::Error)?;
                {
                    let mut data = frame_ref.lock().map_err(|_| gst::FlowError::Error)?;
                    if data.is_empty() {
                        *data = map.to_vec();
                    } else {
                        data.copy_from_slice(&map);
                    }
                }

                {
                    if check_width {
                        let mut lock = renderer.lock().unwrap();

                        if lock.width == 0 {
                            let pad = sink.static_pad("sink").ok_or(gst::FlowError::Error)?;

                            let caps = pad.current_caps().ok_or(gst::FlowError::Error)?;
                            let s = caps.structure(0).ok_or(gst::FlowError::Error)?;
                            let width = s.get::<i32>("width").map_err(|_| gst::FlowError::Error)?;
                            let height =
                                s.get::<i32>("height").map_err(|_| gst::FlowError::Error)?;
                            *lock = Renderer::new(width as _, height as _, lock.term_height);
                            check_width = false; // stop locking every frame after we properly initialize our renderer
                        }
                    }
                }
                match notify.try_send(()) {
                    Err(std::sync::mpsc::TrySendError::Full(_)) => {
                        dropped_frames_counter.fetch_add(1, std::sync::atomic::Ordering::Relaxed); // frame is dropped
                        return Ok(gst::FlowSuccess::Ok);
                    }
                    Err(_) => return Err(gst::FlowError::Error),
                    _ => (),
                }
                frame_counter.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
                Ok(gst::FlowSuccess::Ok)
            })
            .build(),
    );

    if let Err(e) = source.set_state(gst::State::Playing) {
        eprintln!("error playing file: {}", e);
        std::process::exit(1);
    }

    // wait for up to 5 seconds until the decoder gets the source capabilities
    source.state(gst::ClockTime::from_seconds(args.timeout)).0?;
    if !args.kitty {
        if args.ansi256 {
            do_run::<XTerm256>(wait, renderer_clone, frame_ref_clone, out)?;
        } else {
            do_run::<RGB>(wait, renderer_clone, frame_ref_clone, out)?;
        }
    } else {
        while wait.recv_timeout(Duration::from_secs(3)).is_ok() {
            let renderer = renderer_clone.lock().unwrap();
            let mut frame_data = frame_ref_clone.lock().unwrap();
            if frame_data.is_empty() {
                continue;
            }
            renderer.render_meow(&mut frame_data, out.deref_mut())?;
        }
    }

    // end the playback 3 seconds after the video ends

    print_dropped_frames(dropped_frames_counter_2, frame_counter_2, stdout());
    Ok(())
}

fn do_run<C: Colorize + Clone>(
    wait: Receiver<()>,
    renderer: Arc<Mutex<Renderer>>,
    frame_ref_clone: Arc<Mutex<Vec<u8>>>,
    mut out: HideCursor<impl Write>,
) -> Result<(), Box<dyn Error>> {
    let mut differ = {
        let renderer = renderer.lock().unwrap();
        renderer.create_differ()
    };
    while wait.recv_timeout(Duration::from_secs(3)).is_ok() {
        let mut renderer = renderer.lock().unwrap();
        {
            let frame_data = frame_ref_clone.lock().unwrap();
            if frame_data.is_empty() {
                continue;
            }
            renderer.process_frame(&frame_data);
        }
        renderer.render_frame::<C>(out.deref_mut(), &mut differ)?;
    }
    Ok(())
}
