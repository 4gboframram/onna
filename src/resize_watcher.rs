use std::sync::atomic::AtomicBool;
use std::error::Error;

pub trait ResizeWatcher {
    fn resized(&mut self) -> bool;
}

// Use unix signal for efficiency
struct SignalWatcher {
    flag: std::sync::Arc<AtomicBool>
}

#[allow(dead_code)]  // Only used on some platforms
impl SignalWatcher {
    fn new() -> Result<Self, Box<dyn Error>> {
        let flag = std::sync::Arc::new(AtomicBool::new(false));
        signal_hook::flag::register(signal_hook::consts::SIGWINCH, flag.clone())?;

        Ok(SignalWatcher {
            flag
        })
    }
}

impl ResizeWatcher for SignalWatcher {
    fn resized(&mut self) -> bool {
        self.flag.swap(false, std::sync::atomic::Ordering::Relaxed)
    }
}

// ... or poll for changes using termsize
struct PollWatcher {
    width: u16,
    height: u16,
}

#[allow(dead_code)]  // Only used on some platforms
impl PollWatcher {
    fn new() -> Result<Self, Box<dyn Error>> {
        let termsize = termsize::get().unwrap();
        let (termwidth, termheight) = (termsize.cols, termsize.rows);

        Ok(PollWatcher {
            width: termwidth,
            height: termheight
        })
    }
}

impl ResizeWatcher for PollWatcher {
    fn resized(&mut self) -> bool {
        let termsize = termsize::get().unwrap();
        let (termwidth, termheight) = (termsize.cols, termsize.rows);

        let changed = (termwidth != self.width) || (termheight != self.height);

        self.width = termwidth;
        self.height = termheight;

        changed
    }
}

#[cfg(unix)]
pub fn default_watcher() -> Result<impl ResizeWatcher, Box<dyn Error>> {
	SignalWatcher::new()
}

#[cfg(not(unix))]
pub fn default_watcher() -> Result<impl ResizeWatcher, Box<dyn Error>> {
	PollWatcher::new()
}
