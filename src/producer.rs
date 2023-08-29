use gst::prelude::*;
use gstreamer as gst;
use std::{
    fmt::Display,
    sync::{
        atomic::AtomicUsize,
        mpsc::{sync_channel, Receiver, SyncSender},
        Arc, Mutex, MutexGuard,
    },
    time::Duration,
};

use gst_app::AppSink;
use gstreamer_app as gst_app;
pub type Error = Box<dyn std::error::Error>;

#[derive(Debug, Clone)]
pub enum ProducerMessage {
    Initialize { width: u32, height: u32 },
    FrameReady,
}
pub trait Producer {
    fn subscribe(&mut self) -> Receiver<ProducerMessage>;
    fn frame(&self) -> Option<MutexGuard<Vec<u8>>>;
}

#[derive(Debug)]
pub struct FrameCounter {
    pub dropped: AtomicUsize,
    pub not_dropped: AtomicUsize,
}

impl Display for FrameCounter {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let dropped = self.dropped.load(std::sync::atomic::Ordering::SeqCst);
        let not_dropped = self.not_dropped.load(std::sync::atomic::Ordering::SeqCst);
        let total = dropped + not_dropped;
        write!(
            f,
            "dropped {dropped} frames of {total} ({:.2}%)",
            dropped as f32 / total as f32 * 100.
        )
    }
}
#[derive(Debug)]
pub struct GstProducer {
    sink: AppSink,
    caps_filter: gst::Element,
    notify: SyncSender<ProducerMessage>,
    recv: Option<Receiver<ProducerMessage>>,
    frame_data: Arc<Mutex<Vec<u8>>>,
    counter: Arc<FrameCounter>,
}

impl GstProducer {
    pub fn new(pipeline_desc: &str, timeout: Duration) -> Result<Self, Error> {
        let source = gst::parse_launch(pipeline_desc)?;

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
        let app_sink = app_sink.downcast::<AppSink>().unwrap();

        let caps_filter = bin.by_name("caps").unwrap();

        let (notify, recv) = sync_channel(1);
        source.set_state(gst::State::Playing)?;
        source
            .state(gst::ClockTime::from_seconds(timeout.as_secs()))
            .0?;
        let mut this = Self {
            notify,
            caps_filter,
            recv: Some(recv),
            sink: app_sink,
            frame_data: Arc::new(Mutex::new(vec![])),
            counter: Arc::new(FrameCounter {
                dropped: AtomicUsize::new(0),
                not_dropped: AtomicUsize::new(0),
            }),
        };
        this.set_callbacks();
        Ok(this)
    }

    fn set_callbacks(&mut self) {
        let notify = self.notify.clone();
        let frame_data = self.frame_data.clone();
        let counter = self.counter.clone();
        let (mut current_width, mut current_height) = (0, 0);

        self.sink.set_callbacks(
            gst_app::AppSinkCallbacks::builder()
                .new_sample(move |sink| {
                    let sample = sink.pull_sample().map_err(|_| gst::FlowError::Eos)?;
                    let buffer = sample.buffer().ok_or(gst::FlowError::Error)?;
                    let map = buffer.map_readable().map_err(|_| gst::FlowError::Error)?;
                    {
                        let mut data = frame_data.lock().map_err(|_| gst::FlowError::Error)?;
                        // TODO: Optimise, since most frames will be the same size
                        *data = map.to_vec();
                    }

                    {
                        // Get the resolution of this frame using it's accompanying caps
                        let caps = sample.caps().ok_or(gst::FlowError::Error)?;
                        let s = caps.structure(0).ok_or(gst::FlowError::Error)?;
                        let width =
                            s.get::<i32>("width").map_err(|_| gst::FlowError::Error)? as u32;
                        let height =
                            s.get::<i32>("height").map_err(|_| gst::FlowError::Error)? as u32;

                        // If resolution is changed, then the renderer must be re-initialised
                        if width != current_width || height != current_height {

                            notify
                                .send(ProducerMessage::Initialize { width, height })
                                .map_err(|_| gst::FlowError::Error)?;
                            // stop locking every frame after we properly initialize our renderer
                            current_width = width;
                            current_height = height;
                        }
                    }
                    match notify.try_send(ProducerMessage::FrameReady) {
                        Err(std::sync::mpsc::TrySendError::Full(_)) => {
                            counter
                                .dropped
                                .fetch_add(1, std::sync::atomic::Ordering::SeqCst);
                            return Ok(gst::FlowSuccess::Ok);
                        }
                        Err(_) => return Err(gst::FlowError::Error),
                        _ => (),
                    }
                    counter
                        .not_dropped
                        .fetch_add(1, std::sync::atomic::Ordering::SeqCst);
                    Ok(gst::FlowSuccess::Ok)
                })
                .build(),
        )
    }
    pub fn counter(&self) -> Arc<FrameCounter> {
        self.counter.clone()
    }
    pub fn resize(&self, width: u32, height: u32) {
        let mut caps = self.caps_filter.property("caps").unwrap()
            .get::<gst::Caps>().unwrap();
        let new_caps = caps.make_mut();

        let structure = new_caps.structure_mut(0).unwrap();

        structure.set("width", width as i32);
        structure.set("height", height as i32);

        self.caps_filter.set_property("caps", new_caps.to_owned())
            .expect("failed to update resolution");
    }
}

impl Producer for GstProducer {
    fn frame(&self) -> Option<MutexGuard<Vec<u8>>> {
        Some(self.frame_data.lock().unwrap())
    }
    fn subscribe(&mut self) -> Receiver<ProducerMessage> {
        self.recv
            .take()
            .expect("only a single subscriber can be subscribed to this producer")
    }
}
