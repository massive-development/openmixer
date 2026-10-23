//! A library, a track loader, the real `DjEngine` on the mock device and an automix runner, with
//! generated and analysed tracks. Nothing opens a sound device; the test drives the mock device
//! block by block on a simulated 30 Hz clock.
#![allow(dead_code)]

use std::cell::RefCell;
use std::path::{Path, PathBuf};
use std::rc::Rc;
use std::sync::Arc;

use om_analysis::{
    AnalysisContext, Analyzer, AnalyzerRegistry, AudioClip, BeatGridModel, BpmResult, KeyResult,
    MusicalKey, WaveformData, WaveformLevel, WaveformPoint,
};
use om_audio::{AudioDevice, DjEngine, MockAudioDevice};
use om_automix::queue::QueueConfig;
use om_core::{AutomixSink, ParameterRouter, ParameterTarget};
use om_library::{Library, TrackRecord};
use om_testing::signals::sine_amp;
use om_testing::wait::{wait_until, DEFAULT_POLL};
use om_testing::wav::write_pcm16_wav;
use om_ui::library::{LibraryServices, Options, TrackLoader};
use openmixer::automix_host::{AppAutomixHost, AutomixLibrary};
use openmixer::automix_queue::{LibraryQueue, QueueSource};
use openmixer::automix_service::{AutomixRunner, AutomixShared};
use std::time::Duration;

pub const RATE: u32 = 44_100;
/// Simulated control period: 30 Hz.
pub const TICK: f64 = 1.0 / 30.0;
/// Frames the mock device renders per control tick (1470 = 44 100 / 30), in blocks of 210.
const BLOCK: usize = 210;
pub const BLOCKS_PER_TICK: usize = 7;

/// Tempo of every generated track.
pub const BPM: f64 = 120.0;

struct FixedAnalyzer {
    id: &'static str,
    payload: String,
}

impl Analyzer for FixedAnalyzer {
    fn id(&self) -> &str {
        self.id
    }
    fn version(&self) -> i32 {
        1
    }
    fn analyze(&self, _clip: &AudioClip, _context: &dyn AnalysisContext) -> Option<String> {
        Some(self.payload.clone())
    }
}

/// Analysers that give every track a 120 BPM grid, a key, a waveform and a structure with an
/// intro, a main part and an outro (mix-in at the start of the main part).
fn analyzers(seconds: f64) -> Arc<AnalyzerRegistry> {
    let registry = AnalyzerRegistry::new();
    let key = KeyResult {
        key: MusicalKey::new(9, true),
        confidence: 0.9,
        chroma: [0.0; 12],
        tonal: true,
    };
    let bpm = BpmResult {
        bpm: BPM,
        confidence: 0.9,
        grid: BeatGridModel::constant(BPM, f64::from(RATE), 0.0, 4),
    }
    .to_json();
    let third = seconds / 3.0;
    let structure = format!(
        r#"{{"version":1,"onsets":[],"energyHop":0.5,"energy":[],"sections":[
            {{"start":0,"end":{third},"energy":0.3,"label":"intro"}},
            {{"start":{third},"end":{two},"energy":0.9,"label":"main"}},
            {{"start":{two},"end":{seconds},"energy":0.4,"label":"outro"}}],
            "mixIn":{third},"mixOut":{two}}}"#,
        two = 2.0 * third
    );
    let analysers: [(&'static str, String); 4] = [
        ("bpm", bpm),
        ("key", key.to_json()),
        ("waveform", waveform_json()),
        ("structure", structure),
    ];
    for (id, payload) in analysers {
        registry
            .add(Arc::new(FixedAnalyzer { id, payload }))
            .expect("unique ids");
    }
    Arc::new(registry)
}

fn waveform_json() -> String {
    let base = WaveformLevel {
        samples_per_point: 64,
        points: vec![
            WaveformPoint {
                min: -0.5,
                max: 0.5,
                loudness: 0.4,
                centroid: 0.3,
                spread: 0.2,
            };
            64
        ],
    };
    WaveformData::new(f64::from(RATE), 64 * 64, base).to_json()
}

/// The library side of the rig: services and the loader, borrowed together by the host.
pub struct Library3 {
    pub services: LibraryServices,
    pub loader: TrackLoader,
}

impl AutomixLibrary for Library3 {
    fn parts(&self) -> (&LibraryServices, &TrackLoader) {
        (&self.services, &self.loader)
    }
    fn parts_mut(&mut self) -> (&mut LibraryServices, &mut TrackLoader) {
        (&mut self.services, &mut self.loader)
    }
}

pub struct Rig {
    pub dir: tempfile::TempDir,
    pub library: Rc<RefCell<Library3>>,
    pub engine: Arc<DjEngine>,
    pub router: Arc<ParameterRouter>,
    pub automix: Arc<AutomixShared>,
    pub runner: AutomixRunner,
    pub dev: MockAudioDevice,
    pub playlist: i64,
    pub tracks: Vec<i64>,
    pub seconds: f64,
    now: f64,
}

impl Rig {
    /// A rig with `count` tracks of `seconds` each, all analysed, in a playlist in that order.
    pub fn new(count: usize, seconds: f64) -> Self {
        let dir = tempfile::tempdir().expect("tempdir");
        let mut options = Options::for_path(":memory:");
        options.analyzers = Some(analyzers(seconds));
        options.on_error = Some(Box::new(|m| panic!("unexpected library error: {m}")));
        let mut services = LibraryServices::create(options).expect("library opens");
        let loader = TrackLoader::new(&mut services, 2);

        let (engine, audio) = DjEngine::new(2);
        let engine = Arc::new(engine);
        let router = Arc::new(ParameterRouter::new());
        engine.register(&router);
        let automix = AutomixShared::new();
        // Seeks land in wall time on the decoder thread; the simulated clock here runs faster, so
        // the rig allows a second of simulated grace for a cue to show up.
        automix.set_config(om_automix::AutomixConfig {
            cue_grace_s: 1.0,
            ..om_automix::AutomixConfig::default()
        });
        router.register_typed(om_core::param_id::Namespace::Automix, automix.control());
        let observed = Arc::clone(&automix);
        router.set_input_observer(Some(Arc::new(move |id: &om_core::ParamId| {
            observed.notify_user_input(id);
        })));
        let mut dev = MockAudioDevice::new(f64::from(RATE), BLOCK, 0, 2);
        dev.start(Box::new(audio)).expect("start");
        let sink = Arc::new(AutomixSink::new(Arc::clone(&router)));

        let mut tracks = Vec::new();
        for n in 0..count {
            let path = write_track(dir.path(), n, seconds);
            let record = TrackRecord {
                path: path.to_string_lossy().into_owned(),
                title: format!("Track {n}"),
                artist: "Gen".into(),
                bpm: Some(BPM),
                camelot: "8A".into(),
                key: "Am".into(),
                duration_sec: seconds,
                ..TrackRecord::default()
            };
            let id = services.library().tracks().insert(&record).expect("insert");
            tracks.push(id);
        }
        let playlist = services
            .library()
            .playlists()
            .create("Set", None, false)
            .expect("playlist");
        for (i, id) in tracks.iter().enumerate() {
            services
                .library()
                .playlists()
                .add_track(playlist, *id, i as i32)
                .expect("add");
        }
        let library = Rc::new(RefCell::new(Library3 { services, loader }));
        let mut rig = Self {
            dir,
            library,
            engine,
            router,
            automix: Arc::clone(&automix),
            runner: AutomixRunner::new(Arc::clone(&automix), sink),
            dev,
            playlist,
            tracks,
            seconds,
            now: 0.0,
        };
        rig.analyse_all();
        rig
    }

    /// Pumps the services until every track of the rig has its analysis.
    pub fn analyse_all(&mut self) {
        let ids = self.tracks.clone();
        for id in &ids {
            self.library
                .borrow_mut()
                .services
                .prep_queue_mut()
                .enqueue(*id, true)
                .expect("enqueue");
        }
        let ok = wait_until(Duration::from_secs(20), DEFAULT_POLL, || {
            self.pump();
            ids.iter().all(|id| {
                let lib = self.library.borrow();
                let track = lib
                    .services
                    .library()
                    .tracks()
                    .find_by_id(*id)
                    .ok()
                    .flatten();
                track.is_some_and(|t| lib.services.has_beat_grid(&t))
            })
        });
        assert!(ok, "the tracks were never analysed");
    }

    pub fn pump(&mut self) {
        let mut lib = self.library.borrow_mut();
        let Library3 { services, loader } = &mut *lib;
        services.pump();
        loader.pump(&self.engine, services);
    }

    pub fn library(&self) -> Arc<Library> {
        self.library.borrow().services.library_handle()
    }

    /// Loads track `index` of the rig into `deck`.
    pub fn load(&mut self, deck: usize, index: usize) {
        let id = self.tracks[index];
        let mut lib = self.library.borrow_mut();
        let Library3 { services, loader } = &mut *lib;
        loader
            .load(&self.engine, services, deck, id, false)
            .expect("load");
    }

    /// An automix host over this rig, with the playlist as its source.
    pub fn host(&self) -> AppAutomixHost<Library3> {
        let queue = LibraryQueue::shared(
            self.library(),
            QueueSource::Playlist(self.playlist),
            QueueConfig::default(),
            7,
        );
        AppAutomixHost::new(Arc::clone(&self.engine), Rc::clone(&self.library), queue)
    }

    /// Sets the host the runner steps.
    pub fn attach_host(&mut self) {
        let host = self.host();
        self.runner.set_host(Box::new(host));
    }

    /// Renders one control tick of audio and steps the pipeline: the engine's control clock, the
    /// library pump and the automix runner, at the next simulated time.
    pub fn tick(&mut self) {
        // Seeks are acknowledged by the decoder thread in wall time, so the simulated clock is
        // paced to let a cue land within the controller's grace (a few ticks). Without the pause a
        // cue would look moved by the user.
        std::thread::sleep(Duration::from_millis(2));
        self.dev.render_blocks(BLOCKS_PER_TICK);
        self.engine.update_control(TICK);
        self.pump();
        self.now += TICK;
        self.runner.step(&self.engine, self.now);
    }

    pub fn set(&self, id: &str, value: f64) {
        assert!(self.router.set_parameter(id, value), "{id} not routed");
    }

    pub fn time(&self) -> f64 {
        self.now
    }

    /// The path a track file lives at (tests check the decoded length).
    pub fn file(&self, index: usize) -> PathBuf {
        self.dir.path().join(format!("track{index}.wav"))
    }
}

fn write_track(dir: &Path, n: usize, seconds: f64) -> PathBuf {
    let frames = (seconds * f64::from(RATE)) as usize;
    let freq = 110.0 + 25.0 * n as f64;
    let pcm: Vec<i16> = sine_amp(freq, 0.4, f64::from(RATE), frames)
        .into_iter()
        .flat_map(|s| {
            let v = (s * f32::from(i16::MAX)) as i16;
            [v, v]
        })
        .collect();
    let path = dir.join(format!("track{n}.wav"));
    write_pcm16_wav(&path, 2, RATE, &pcm);
    path
}
