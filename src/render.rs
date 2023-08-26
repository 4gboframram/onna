use std::{
    io::{self, Write},
    marker::PhantomData,
    ops::Range,
    slice::from_raw_parts,
};

use base64ct::{Base64, Encoding};

use crate::{
    buffer::Differ,
    color::{Ansi256, BackgroundAnsi256, BackgroundRgb, Colorize, Rgb},
};

pub type Pixel = [u8; 4];

pub trait Renderer {
    type State;
    fn from_dims(width: u32, height: u32) -> Self;
    fn create_state(&self) -> Self::State;
    fn width(&self) -> u32;
    fn height(&self) -> u32;
    fn consume(&mut self, data: &[Pixel]);

    fn render_frame(&self, output: &mut impl Write, state: &mut Self::State) -> io::Result<()>;

    fn verify_input<'a>(&self, data: &'a [u8]) -> &'a [Pixel] {
        let area = self.width() * self.height();
        assert_eq!(data.len() as u32, area * 4);
        let ptr = data.as_ptr().cast::<[u8; 4]>();
        let data = unsafe { from_raw_parts(ptr, area as _) };
        data
    }
}

pub struct DefaultRenderer<C: Colorize> {
    width: u32,
    height: u32,

    // [r, g, b, char]
    color_buf: Box<[Pixel]>,
    prev_buf: Box<[Pixel]>,
    _phantom: PhantomData<C>,
}

impl<C: Colorize> DefaultRenderer<C> {
    pub fn new(width: u32, height: u32) -> Self {
        let num_pixels = width * height;
        let color_buf = vec![[0u8, 0, 0, 0]; num_pixels as usize].into_boxed_slice();

        Self {
            width,
            height,

            prev_buf: color_buf.clone(),
            color_buf,
            _phantom: PhantomData,
        }
    }
}

macro_rules! impl_fg {
    ([$($ty:ty),*]) => {
        $(impl Renderer for DefaultRenderer<$ty> {
            type State = Differ<$ty>;
            fn from_dims(width: u32, height: u32) -> Self { Self::new(width, height) }
            fn create_state(&self) -> Self::State {
                Differ::new(self.width, self.height)
            }
            fn width(&self) -> u32 {
                self.width
            }
            fn height(&self) -> u32 {
                self.height
            }
            fn consume(&mut self, data: &[Pixel]) {
                 std::mem::swap(&mut self.color_buf, &mut self.prev_buf);
                for (i, pixel) in data.iter().enumerate() {
                    let lum = luminance(*pixel);
                    let index = lum >> 2;
                    let mut pixel = pixel.clone();
                    pixel[3] = ASCII_CHARS.as_bytes()[index as usize];
                    self.color_buf[i] = gamma_correct(pixel);
                }
            }
            fn render_frame(
                &self,
                output: &mut impl Write,
                state: &mut Self::State,
            ) -> io::Result<()> {

                // profiling suggests that we are almost 100% io-bound, so we are basically free to do any optimization on escape sequences
                state.assign_diff(&self.color_buf, &self.prev_buf);

                let mut prev_end: usize = 0;
                let mut prev_color = <$ty>::default();

                for (i, (pos, color, chr)) in state.data().iter().enumerate() {
                    render_stride(
                        i,
                        pos,
                        color,
                        chr,
                        &mut prev_end,
                        &mut prev_color,
                        output,
                        self.width,
                    )?;
                }

                output.flush()?;
                Ok(())
            }
        })+
    };
}

macro_rules! impl_bg {
    ([$($ty:ty),*]) => {
        $(impl Renderer for DefaultRenderer<$ty> {
            type State = Differ<$ty>;
            fn from_dims(width: u32, height: u32) -> Self { Self::new(width, height) }
            fn create_state(&self) -> Self::State {
                Differ::new(self.width, self.height)
            }
            fn width(&self) -> u32 {
                self.width
            }
            fn height(&self) -> u32 {
                self.height
            }
            fn consume(&mut self, data: &[Pixel]) {
                std::mem::swap(&mut self.color_buf, &mut self.prev_buf);
                // apply no filters. just a memcpy
                self.color_buf.copy_from_slice(data)
            }
            fn render_frame(
                &self,
                output: &mut impl Write,
                state: &mut Self::State,
            ) -> io::Result<()> {


                // profiling suggests that we are almost 100% io-bound, so we are basically free to do any optimization on escape sequences
                state.assign_diff(&self.color_buf, &self.prev_buf);

                let mut prev_end: usize = 0;
                let mut prev_color = <$ty>::default();

                 for (i, (pos, color, _)) in state.data().iter().enumerate() {
                    render_stride(
                        i,
                        pos,
                        color,
                        &b' ',
                        &mut prev_end,
                        &mut prev_color,
                        output,
                        self.width,
                    )?;
                }

                output.flush()?;
                Ok(())
            }
        })+
    };
}

impl_fg!([Ansi256, Rgb]);
impl_bg!([BackgroundAnsi256, BackgroundRgb]);

// original 70 character gradient
// const ASCII_CHARS: &str =
//     "$@B%8&WM#*oahkbdpqwmZO0QLCJUYXzcvunxrjft/\\|()1{}[]?-_+~<>i!lI;:,\"^`'. ";

// reversed for light mode
// const ASCII_CHARS: &str = "$@B%8&W#*oahkbdpqwmZOQCJUYXzcvuxrjft/\\|()1{}[]?-_+~<>i!lI;:,\"`. ";
const ASCII_CHARS: &str = " .`\",:;Il!i><~+_-?][}{1)(|\\/tfjrxuvczXYUJCQOZmwqpdbkhao*#W&8%B@$";

const fn luminance(pixel: [u8; 4]) -> u8 {
    let [r, g, b, _] = pixel;
    (((r as u32) * 3 + (b as u32) + ((g as u32) << 2)) >> 3) as u8
}

// #[allow(clippy::cast_possible_truncation)]
// fn normalize_luminance(pixel: [u8; 4], luminance: u8) -> [u8; 4] {
//     let [r, g, b, ch] = pixel;
//     //let lum = F::from_bits(luminance as u32 + 1);
//     let lum = (luminance as f32 / 255.);
//     let r = r as f32 / 255.;
//     let g = g as f32 / 255.;
//     let b = b as f32 / 255.;

//     let r = (r.powf(lum) * 255.0).min(u8::MAX as f32) as u8;
//     let g = (g.powf(lum) * 255.0).min(u8::MAX as f32) as u8;
//     let b = (b.powf(lum) * 255.0).min(u8::MAX as f32) as u8;
//     [r, g, b, ch]
// }
#[allow(clippy::cast_possible_truncation)]
fn gamma_correct(pixel: Pixel) -> Pixel {
    let [r, g, b, c] = pixel;
    const GAMMA: f32 = 0.5;
    let r = r as f32 / 255.;
    let g = g as f32 / 255.;
    let b = b as f32 / 255.;
    let r = (r.powf(GAMMA) * 255.).min(u8::MAX as _) as u8;
    let g = (g.powf(GAMMA) * 255.).min(u8::MAX as _) as u8;
    let b = (b.powf(GAMMA) * 255.).min(u8::MAX as _) as u8;
    [r, g, b, c]
}
fn render_stride<C: Colorize>(
    i: usize,
    pos: &Range<usize>,
    color: &C,
    chr: &u8,
    prev_end: &mut usize,
    prev_color: &mut C,
    mut output: &mut impl Write,
    width: u32,
) -> io::Result<()> {
    // If the previous end is the same as the start, that means the cursor is in the right position
    // and therefore we do not need to print the escape to skip to the line,
    // unless the requred position *is* the origin.
    // In that case, we almost always need to jump to it.
    if &pos.start != prev_end || prev_end == &0 {
        let line = pos.start / width as usize;
        let column = pos.start % width as usize;
        // it is almost always less characters to skip directly to the line and column than to use relative motion
        // maybe i'll optimize that too
        write!(output, "\x1b[{};{}H", line, column)?;
    }
    if color != prev_color || i == 0 {
        color.write_escape(&mut output)?;
    }

    let mut is_first = true;
    for i in pos.clone() {
        let col = i % width as usize;
        if col == 0 && !is_first {
            output.write_all(b"\n")?;
        }
        output.write_all(&[*chr])?;
        is_first = false;
    }
    *prev_end = pos.end;
    *prev_color = color.clone();
    Ok(())
}

pub struct KittyRenderer {
    width: u32,
    height: u32,
    encoded: String,
}

impl Renderer for KittyRenderer {
    type State = ();
    fn from_dims(width: u32, height: u32) -> Self {
        let len = width as usize * height as usize * 4;
        fn ceiling_div(x: usize, y: usize) -> usize {
            (x + y - 1) / y
        }
        let base64_encoded_len = 4 * ceiling_div(len, 3);
        Self {
            width,
            height,
            encoded: String::from_utf8(vec![0u8; base64_encoded_len]).unwrap(),
        }
    }
    fn width(&self) -> u32 {
        self.width
    }
    fn height(&self) -> u32 {
        self.height
    }
    fn create_state(&self) -> Self::State {}
    fn consume(&mut self, data: &[Pixel]) {
        let ptr = data.as_ptr().cast::<u8>();
        let slice = unsafe { from_raw_parts(ptr, self.width as usize * self.height as usize * 4) };
        Base64::encode(slice, unsafe { self.encoded.as_bytes_mut() }).unwrap();
    }
    fn render_frame(&self, output: &mut impl Write, _state: &mut Self::State) -> io::Result<()> {
        let mut iter = self.encoded.as_bytes().chunks(4096).peekable();
        while let Some(chunk) = iter.next() {
            let m = iter.peek().is_some() as u8;
            // let s = Base64::encode_string(chunk);
            let s = std::str::from_utf8(chunk).unwrap();
            write!(
                output,
                "\x1b_Ga=T,f=32,s={},v={},C=1,m={},x=1,y=1;{s}\x1b\\",
                self.width, self.height, m
            )?;
        }
        output.flush()
    }
}
