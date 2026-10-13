// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) ibis inc.
// Additional terms under AGPL-3.0 section 7(b): see NOTICE.
//! 動画・音声の再生 on Linux: a clip played by GStreamer's `playbin`, its
//! sound going to the system's default output. GPUI draws surfaces on macOS
//! only, so a video's frames are taken from an `appsink` as BGRA pixels and
//! drawn as images.
//!
//! GPUI keeps every image it draws in its windows' sprite atlases until it
//! is told to drop it, and a playing video is a new image every frame: the
//! frames no longer shown are noted here as they go, and `release_frames`
//! takes them out of the atlases.

use std::cell::Cell;
use std::path::Path;
use std::sync::{Arc, Mutex, OnceLock};
use std::time::Duration;

use gpui_kit::{App, ImageSource, RenderImage};
use gst::glib;
use gst::prelude::*;
use gst_video::prelude::*;
use gstreamer as gst;
use gstreamer_app as gst_app;
use gstreamer_video as gst_video;

/// How long a clip may take to get ready to play (its first frame and sound
/// decoded) before it is given up.
const OPEN_TIMEOUT: Duration = Duration::from_secs(5);

/// Whether GStreamer is there to play with: started when the first clip is
/// opened, and said once when it cannot be.
fn ready() -> bool {
    static READY: OnceLock<bool> = OnceLock::new();
    *READY.get_or_init(|| match gst::init() {
        Ok(()) => true,
        Err(err) => {
            log::warn!("GStreamer cannot be started, so videos and audio are not played: {err}");
            false
        }
    })
}

/// A video frame, as GPUI draws it here: an image of its pixels.
#[derive(Clone)]
pub(crate) struct Frame(Arc<Shown>);

/// A frame handed out to be shown; noted in `RETIRED` when the last handle
/// on it goes, however it goes (replaced by the next frame, its clip
/// stopped, its presentation closed).
struct Shown(Arc<RenderImage>);

/// The frames no longer shown that may still be in a window's atlas.
static RETIRED: Mutex<Vec<Arc<RenderImage>>> = Mutex::new(Vec::new());

impl Drop for Shown {
    fn drop(&mut self) {
        if let Ok(mut retired) = RETIRED.lock() {
            retired.push(self.0.clone());
        }
    }
}

impl From<Frame> for ImageSource {
    fn from(frame: Frame) -> Self {
        ImageSource::Render(frame.0.0.clone())
    }
}

/// The frames no longer shown taken out of every window's sprite atlas.
/// Called between the windows' updates (from the clips' timer), when GPUI
/// can reach all of them.
pub(crate) fn release_frames(cx: &mut App) {
    let retired = RETIRED.lock().map(|mut retired| std::mem::take(&mut *retired)).unwrap_or_default();
    for image in retired {
        cx.drop_image(image, None);
    }
}

/// The newest frame decoded and not taken yet, and the width over the
/// height of the video as it is shown.
#[derive(Default)]
struct Newest {
    image: Option<Arc<RenderImage>>,
    aspect: Option<f64>,
}

/// The frame in `sample` as GPUI's image (BGRA, its rows without the
/// padding they may end with).
fn picture(sample: &gst::Sample) -> Option<Newest> {
    let info = gst_video::VideoInfo::from_caps(sample.caps()?).ok()?;
    let frame = gst_video::VideoFrameRef::from_buffer_ref_readable(sample.buffer()?, &info).ok()?;
    let (width, height) = (info.width(), info.height());
    let (row, stride) = (width as usize * 4, usize::try_from(frame.plane_stride()[0]).ok()?);
    if width == 0 || height == 0 || stride < row {
        return None;
    }
    let data = frame.plane_data(0).ok()?;
    let mut pixels = Vec::with_capacity(row * height as usize);
    for line in data.chunks(stride).take(height as usize) {
        pixels.extend_from_slice(line.get(..row)?);
    }
    let image = image::RgbaImage::from_raw(width, height, pixels)?;
    // The pixels of some videos are not square: the picture is as wide as they make it.
    let par = info.par();
    let aspect = (width as f64 * par.numer() as f64) / (height as f64 * par.denom() as f64);
    Some(Newest { image: Some(Arc::new(RenderImage::new(vec![image::Frame::new(image)]))), aspect: (aspect.is_finite() && aspect > 0.0).then_some(aspect) })
}

/// The frame of `sample` kept as the newest one (on the video sink's thread).
fn keep(newest: &Mutex<Newest>, sample: Result<gst::Sample, glib::BoolError>) -> Result<gst::FlowSuccess, gst::FlowError> {
    let sample = sample.map_err(|_| gst::FlowError::Eos)?;
    if let Some(frame) = picture(&sample)
        && let Ok(mut newest) = newest.lock()
    {
        *newest = frame;
    }
    Ok(gst::FlowSuccess::Ok)
}

/// The width over the height of the video in `path`, if it has a video track.
pub(crate) fn video_aspect(path: &Path) -> Option<f64> {
    // Its first frame says it; no sound output is opened for this.
    let player = Player::start(path, true, false)?;
    let aspect = player.newest.lock().ok()?.aspect;
    aspect
}

/// A clip being played.
pub(crate) struct Player {
    playbin: gst::Element,
    /// Put here by the video sink's thread.
    newest: Arc<Mutex<Newest>>,
    /// It played to its end (or an error stopped it), until it is sought.
    ended: Cell<bool>,
    /// Where it was when last asked, for while it cannot say (a seek under way).
    position: Cell<u32>,
}

impl Player {
    /// The clip in `path`, ready to play; `video` takes its frames too.
    pub(crate) fn open(path: &Path, video: bool) -> Option<Player> {
        Self::start(path, video, true)
    }

    /// The clip in `path` paused at its start, with its first frame and
    /// sound decoded (so it is known to play, and can be sought at once);
    /// `video` and `audio`: which of its streams are played.
    fn start(path: &Path, video: bool, audio: bool) -> Option<Player> {
        if !ready() {
            return None;
        }
        let name = path.display();
        let Some(uri) = std::path::absolute(path).ok().and_then(|path| glib::filename_to_uri(path, None).ok()) else {
            log::warn!("{name}: not a path GStreamer can open");
            return None;
        };
        let playbin = match gst::ElementFactory::make("playbin").build() {
            Ok(playbin) => playbin,
            Err(err) => {
                log::warn!("GStreamer has no playbin (are its base plugins installed?): {err}");
                return None;
            }
        };
        // Only the streams asked for, without subtitles drawn on the
        // picture. The volume is applied in the pipeline: the output's own
        // is the one the desktop remembers for the app.
        let flags = playbin.find_property("flags").and_then(|flags| glib::FlagsClass::with_type(flags.value_type())).and_then(|class| {
            let mut flags = class.builder().set_by_nick("soft-volume");
            // Decoded on the processor (where GStreamer can be told so, from
            // 1.18): the frames are wanted in memory anyway, and a graphics
            // card's decoder takes several times longer to start and to
            // stop, which the window waits for.
            if class.value_by_nick("force-sw-decoders").is_some() {
                flags = flags.set_by_nick("force-sw-decoders");
            }
            if video {
                flags = flags.set_by_nick("video").set_by_nick("deinterlace");
            }
            if audio {
                flags = flags.set_by_nick("audio");
            }
            flags.build()
        });
        let Some(flags) = flags else {
            log::warn!("{name}: this GStreamer's playbin cannot be set up");
            return None;
        };
        playbin.set_property_from_value("flags", &flags);
        playbin.set_property("uri", uri.as_str());
        let newest = Arc::new(Mutex::new(Newest::default()));
        if video {
            let caps = gst_video::VideoCapsBuilder::new().format(gst_video::VideoFormat::Bgra).build();
            let (first, next) = (newest.clone(), newest.clone());
            let callbacks = gst_app::AppSinkCallbacks::builder()
                // The frame it is paused on (at the start, after a seek), and each frame as its time comes.
                .new_preroll(move |sink| keep(&first, sink.pull_preroll()))
                .new_sample(move |sink| keep(&next, sink.pull_sample()))
                .build();
            // Only the newest frame is of use: one not taken yet gives way to it.
            let sink = gst_app::AppSink::builder().caps(&caps).max_buffers(1).drop(true).enable_last_sample(false).callbacks(callbacks).build();
            playbin.set_property("video-sink", &sink);
        }
        // From here on dropping the player stops the pipeline.
        let player = Player { playbin, newest, ended: Cell::new(false), position: Cell::new(0) };
        if let Err(err) = player.playbin.set_state(gst::State::Paused) {
            // Why (no such file, say) is on the pipeline's bus.
            let why = player.playbin.bus().and_then(|bus| bus.pop_filtered(&[gst::MessageType::Error]));
            match why.as_ref().map(|why| why.view()) {
                Some(gst::MessageView::Error(why)) => log::warn!("{name}: GStreamer cannot play it: {}", why.error()),
                _ => log::warn!("{name}: GStreamer cannot play it: {err}"),
            }
            return None;
        }
        player.wait_until_ready(&name, video).then_some(player)
    }

    /// Wait for the pipeline to be paused on the clip's start. `false` when
    /// it cannot be played: not a clip, or no plugin for what it holds.
    fn wait_until_ready(&self, name: &dyn std::fmt::Display, video: bool) -> bool {
        let Some(bus) = self.playbin.bus() else { return false };
        let started = crate::clock::Instant::now();
        let mut missing = false;
        loop {
            let left = OPEN_TIMEOUT.saturating_sub(started.elapsed());
            let wanted = [gst::MessageType::AsyncDone, gst::MessageType::Error, gst::MessageType::Element];
            let Some(message) = bus.timed_pop_filtered(gst::ClockTime::from_mseconds(left.as_millis() as u64), &wanted) else {
                log::warn!("{name}: GStreamer did not get it ready to play in {} s", OPEN_TIMEOUT.as_secs());
                return false;
            };
            match message.view() {
                gst::MessageView::AsyncDone(_) => break,
                gst::MessageView::Error(err) => {
                    log::warn!("{name}: GStreamer cannot play it: {}", err.error());
                    return false;
                }
                // A stream with no decoder installed: the others still play, so it is told here.
                gst::MessageView::Element(element) => {
                    let Some(plugin) = element.structure().filter(|s| s.name() == "missing-plugin") else { continue };
                    log::warn!("{name}: GStreamer has no plugin for {}", plugin.get::<&str>("name").unwrap_or("one of its streams"));
                    missing = true;
                    // An audio clip without its sound is nothing.
                    let sound = plugin.get::<gst::Caps>("detail").is_ok_and(|caps| caps.structure(0).is_some_and(|s| s.name().starts_with("audio/")));
                    if sound && !video {
                        return false;
                    }
                }
                _ => {}
            }
        }
        // A video whose picture cannot be decoded is not played for its sound alone.
        let pictured = self.newest.lock().is_ok_and(|newest| newest.image.is_some());
        !(video && missing && !pictured)
    }

    fn set_state(&self, state: gst::State) {
        if let Err(err) = self.playbin.set_state(state) {
            log::warn!("GStreamer cannot go to {state:?}: {err}");
        }
    }

    pub(crate) fn play(&self) {
        self.set_state(gst::State::Playing);
    }

    pub(crate) fn pause(&self) {
        self.set_state(gst::State::Paused);
    }

    /// Paused and back at the start.
    pub(crate) fn rewind(&self) {
        self.pause();
        self.seek_to_ms(0);
    }

    /// Seek to time in milliseconds.
    pub(crate) fn seek_to_ms(&self, ms: u32) {
        // What the pipeline said before (that it ended) no longer holds.
        if let Some(bus) = self.playbin.bus() {
            bus.set_flushing(true);
            bus.set_flushing(false);
        }
        self.ended.set(false);
        // To the very frame, not the key frame before it: a trimmed clip starts where it was trimmed.
        match self.playbin.seek_simple(gst::SeekFlags::FLUSH | gst::SeekFlags::ACCURATE, gst::ClockTime::from_mseconds(ms as u64)) {
            Ok(()) => self.position.set(ms),
            Err(err) => log::warn!("GStreamer cannot seek to {ms} ms: {err}"),
        }
    }

    /// Current playback time in milliseconds.
    pub(crate) fn current_time_ms(&self) -> u32 {
        if let Some(now) = self.playbin.query_position::<gst::ClockTime>() {
            self.position.set(now.mseconds().min(u32::MAX as u64) as u32);
        }
        self.position.get()
    }

    /// The clip's length in milliseconds.
    #[cfg(test)]
    fn duration_ms(&self) -> Option<u32> {
        self.playbin.query_duration::<gst::ClockTime>().map(|all| all.mseconds() as u32)
    }

    pub(crate) fn is_playing(&self) -> bool {
        // Where it is going, if it is on its way.
        let (_, now, next) = self.playbin.state(gst::ClockTime::ZERO);
        (if next == gst::State::VoidPending { now } else { next }) == gst::State::Playing
    }

    pub(crate) fn set_volume(&self, volume: f32) {
        self.playbin.set_property("volume", volume.clamp(0.0, 1.0) as f64);
    }

    /// Whether it played to its end.
    pub(crate) fn finished(&self) -> bool {
        // GPUI's loop does not run GLib's, so what the pipeline tells waits on its bus to be read here.
        if let Some(bus) = self.playbin.bus() {
            while let Some(message) = bus.pop() {
                match message.view() {
                    gst::MessageView::Eos(_) => self.ended.set(true),
                    // It cannot go on: as its end, so that its poster comes back.
                    gst::MessageView::Error(err) => {
                        log::warn!("GStreamer stopped playing a clip: {}", err.error());
                        self.ended.set(true);
                    }
                    _ => {}
                }
            }
        }
        self.ended.get()
    }

    /// Whether it played to its end, taking trim_end_ms into account.
    pub(crate) fn finished_with_trim(&self, trim_end_ms: u32) -> bool {
        if trim_end_ms > 0 && self.current_time_ms() >= trim_end_ms {
            return true;
        }
        self.finished()
    }

    /// The newest frame, if there is one that was not taken yet.
    pub(crate) fn frame(&self) -> Option<Frame> {
        let image = self.newest.lock().ok()?.image.take()?;
        Some(Frame(Arc::new(Shown(image))))
    }
}

impl Drop for Player {
    fn drop(&mut self) {
        // Its threads stopped and the sound output given back.
        let _ = self.playbin.set_state(gst::State::Null);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    /// Two seconds of a red picture, 320 by 240 at 30 frames a second, with
    /// a quiet tone: Motion JPEG and PCM in a QuickTime file, which
    /// GStreamer's "good" plugins write and read.
    const VIDEO: &str = "videotestsrc pattern=red num-buffers=60 ! video/x-raw,width=320,height=240,framerate=30/1 ! videoconvert ! jpegenc ! queue ! qtmux name=mux ! filesink name=out \
        audiotestsrc samplesperbuffer=441 num-buffers=200 volume=0.05 ! audio/x-raw,format=S16LE,rate=44100,channels=1 ! queue ! mux.";
    /// Two seconds of the tone alone.
    const AUDIO: &str = "audiotestsrc samplesperbuffer=441 num-buffers=200 volume=0.05 ! audio/x-raw,format=S16LE,rate=44100,channels=1 ! wavenc ! filesink name=out";

    /// A clip made for a test, removed after it.
    struct Clip(PathBuf);

    impl Drop for Clip {
        fn drop(&mut self) {
            let _ = std::fs::remove_file(&self.0);
        }
    }

    /// Where a test's clip `name` goes.
    fn clip_path(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("ibisOffice-media-test-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        dir.join(name)
    }

    /// The clip `name` written by GStreamer running `pipeline` (which ends
    /// in `filesink name=out`). `None`, with the reason printed, where
    /// GStreamer cannot: the test is skipped.
    fn record(name: &str, pipeline: &str) -> Option<Clip> {
        if !ready() {
            eprintln!("skipped: GStreamer cannot be started");
            return None;
        }
        let pipeline = match gst::parse::launch(pipeline) {
            Ok(pipeline) => pipeline,
            Err(err) => {
                eprintln!("skipped: GStreamer cannot make the test clip: {err}");
                return None;
            }
        };
        let clip = Clip(clip_path(name));
        pipeline.downcast_ref::<gst::Bin>().unwrap().by_name("out").unwrap().set_property("location", clip.0.to_str().unwrap());
        pipeline.set_state(gst::State::Playing).unwrap();
        let end = pipeline.bus().unwrap().timed_pop_filtered(gst::ClockTime::from_seconds(30), &[gst::MessageType::Eos, gst::MessageType::Error]);
        pipeline.set_state(gst::State::Null).unwrap();
        assert!(end.is_some_and(|end| end.type_() == gst::MessageType::Eos), "the test clip was written");
        Some(clip)
    }

    /// Wait until `done` says so (ten seconds at most: the machine may be busy).
    fn wait_until(what: &str, mut done: impl FnMut() -> bool) {
        let started = crate::clock::Instant::now();
        while !done() {
            assert!(started.elapsed() < Duration::from_secs(10), "waited too long for {what}");
            std::thread::sleep(Duration::from_millis(5));
        }
    }

    fn size(frame: &Frame) -> (i32, i32) {
        let size = frame.0.0.size(0);
        (size.width.0, size.height.0)
    }

    #[test]
    fn a_video_plays_pauses_seeks_and_ends() {
        let Some(clip) = record("video.mov", VIDEO) else { return };
        assert_eq!(video_aspect(&clip.0), Some(320.0 / 240.0));
        let player = Player::open(&clip.0, true).expect("the clip opens");
        let length = player.duration_ms().expect("its length is known");
        assert!((1950..=2050).contains(&length), "two seconds long: {length} ms");

        // Paused at its start, on its first frame.
        assert!(!player.is_playing() && !player.finished());
        assert_eq!(player.current_time_ms(), 0);
        let first = player.frame().expect("the first frame");
        assert_eq!(size(&first), (320, 240));
        // BGRA, as GPUI's images are: the red picture's blue, green, red and alpha.
        let pixel = &first.0.0.as_bytes(0).unwrap()[..4];
        assert!(pixel[0] < 40 && pixel[1] < 40 && pixel[2] > 215 && pixel[3] == 255, "a red pixel: {pixel:?}");
        assert!(player.frame().is_none(), "a frame is taken once");

        // Played: the time goes on and frames come.
        player.set_volume(0.0);
        player.play();
        assert!(player.is_playing());
        let mut frames = 0;
        wait_until("half a second played", || {
            if let Some(frame) = player.frame() {
                assert_eq!(size(&frame), (320, 240));
                frames += 1;
            }
            player.current_time_ms() >= 500
        });
        assert!(frames >= 5, "frames came as it played: {frames}");

        // Paused: the time stands.
        player.pause();
        assert!(!player.is_playing());
        std::thread::sleep(Duration::from_millis(200));
        let paused_at = player.current_time_ms();
        assert!((500..1500).contains(&paused_at), "paused at {paused_at} ms");
        std::thread::sleep(Duration::from_millis(200));
        assert_eq!(player.current_time_ms(), paused_at);

        // Sought while paused: there at once, and the frame of that moment comes.
        while player.frame().is_some() {}
        player.seek_to_ms(1500);
        assert_eq!(player.current_time_ms(), 1500);
        wait_until("the frame sought", || player.frame().is_some());
        assert_eq!(player.current_time_ms(), 1500);
        assert!(!player.finished());

        // Played to its end.
        player.play();
        wait_until("its end", || player.finished());
        assert!(player.finished_with_trim(0));

        // 巻き戻し, as a loop does: paused at the start, and played again.
        player.rewind();
        assert!(!player.finished() && !player.is_playing());
        assert_eq!(player.current_time_ms(), 0);
        player.play();
        wait_until("playing again", || player.current_time_ms() >= 300);
        // A trimmed clip is over at the trim's end.
        assert!(player.finished_with_trim(200));
        assert!(!player.finished());

        // Sought while playing (to a bookmark): it goes on from there, and is never said to be before it.
        player.seek_to_ms(1000);
        let started = crate::clock::Instant::now();
        while started.elapsed() < Duration::from_millis(400) {
            let now = player.current_time_ms();
            assert!((1000..1900).contains(&now), "at {now} ms after seeking to 1000 ms");
        }
        assert!(player.is_playing() && player.current_time_ms() > 1100);
    }

    #[test]
    fn audio_plays_to_its_end() {
        let Some(clip) = record("tone.wav", AUDIO) else { return };
        let player = Player::open(&clip.0, false).expect("the clip opens");
        let length = player.duration_ms().expect("its length is known");
        assert!((1950..=2050).contains(&length), "two seconds long: {length} ms");
        player.set_volume(0.0);
        player.seek_to_ms(1000);
        player.play();
        wait_until("its end", || player.finished());
        assert!(player.current_time_ms() >= 1900);
        assert!(player.frame().is_none(), "no frames of a sound");
        // No picture to take the shape of.
        assert_eq!(video_aspect(&clip.0), None);
    }

    #[test]
    fn what_is_not_a_clip_does_not_open() {
        if !ready() {
            return eprintln!("skipped: GStreamer cannot be started");
        }
        let text = Clip(clip_path("text.mp4"));
        std::fs::write(&text.0, "not a video\n".repeat(100)).unwrap();
        assert!(Player::open(&text.0, true).is_none());
        assert!(Player::open(&text.0, false).is_none());
        assert_eq!(video_aspect(&text.0), None);
        let missing = clip_path("missing.mp4");
        assert!(Player::open(&missing, true).is_none());
        assert_eq!(video_aspect(&missing), None);
    }

    /// The clip of `VIDEO` with the codec `fourcc` of one of its tracks
    /// renamed to one no plugin decodes.
    fn with_unknown_codec(name: &str, fourcc: &[u8; 4]) -> Option<Clip> {
        let whole = record(&format!("whole-{name}"), VIDEO)?;
        let mut data = std::fs::read(&whole.0).unwrap();
        // The track's sample description (stsd) names its codec.
        let at = data.windows(4).position(|w| w == fourcc).expect("the codec's name in the file");
        assert!(data[at - 24..at].windows(4).any(|w| w == b"stsd"), "named in the sample description");
        data[at..at + 4].copy_from_slice(b"zzzz");
        let clip = Clip(clip_path(name));
        std::fs::write(&clip.0, data).unwrap();
        Some(clip)
    }

    #[test]
    fn a_clip_without_its_plugin_does_not_open() {
        // No plugin for the picture: not played for its sound alone, but its sound is one.
        let Some(clip) = with_unknown_codec("no-picture.mov", b"jpeg") else { return };
        assert!(Player::open(&clip.0, true).is_none());
        assert_eq!(video_aspect(&clip.0), None);
        assert!(Player::open(&clip.0, false).is_some());
        // No plugin for the sound: the picture plays silent, and it is no sound.
        let Some(clip) = with_unknown_codec("no-sound.mov", b"sowt") else { return };
        assert!(Player::open(&clip.0, true).is_some_and(|player| player.frame().is_some()));
        assert!(Player::open(&clip.0, false).is_none());
    }
}
