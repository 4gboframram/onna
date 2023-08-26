use std::io::Write;
pub trait Colorize: PartialEq + Default + Clone {
    fn from_rgb(rgb: [u8; 3]) -> Self;
    fn write_escape(&self, out: &mut impl Write) -> std::io::Result<()>;
}

#[derive(Clone, Copy, PartialEq, Default)]
pub struct Rgb([u8; 3]);

impl Colorize for Rgb {
    fn from_rgb(rgb: [u8; 3]) -> Self {
        Self(rgb)
    }
    fn write_escape(&self, out: &mut impl Write) -> std::io::Result<()> {
        let Self([r, g, b]) = *self;
        write!(out, "\x1b[38;2;{r};{g};{b}m")
    }
}

#[derive(Clone, Copy, PartialEq, Default)]
pub struct BackgroundRgb([u8; 3]);

impl Colorize for BackgroundRgb {
    fn from_rgb(rgb: [u8; 3]) -> Self {
        Self(rgb)
    }
    fn write_escape(&self, out: &mut impl Write) -> std::io::Result<()> {
        let Self([r, g, b]) = *self;
        write!(out, "\x1b[48;2;{r};{g};{b}m")
    }
}

#[derive(Clone, Copy, PartialEq, Default)]
pub struct Ansi256(u8);

impl Colorize for Ansi256 {
    fn from_rgb(rgb: [u8; 3]) -> Self {
        Self(ansi_colours::ansi256_from_rgb(rgb))
    }
    fn write_escape(&self, out: &mut impl Write) -> std::io::Result<()> {
        let ansi = self.0;
        write!(out, "\x1b[38;5;{ansi}m")
    }
}

#[derive(Clone, Copy, PartialEq, Default)]
pub struct BackgroundAnsi256(u8);

impl Colorize for BackgroundAnsi256 {
    fn from_rgb(rgb: [u8; 3]) -> Self {
        Self(ansi_colours::ansi256_from_rgb(rgb))
    }
    fn write_escape(&self, out: &mut impl Write) -> std::io::Result<()> {
        let ansi = self.0;
        write!(out, "\x1b[48;5;{ansi}m")
    }
}
