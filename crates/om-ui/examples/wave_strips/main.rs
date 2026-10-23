//! Tuning tool for the spectrum waveform colour (E10): analyses audio files and renders the
//! overview and three zoomed strips to PNG, per theme, with the hue statistics the hue-range test
//! uses. Nothing is played and nothing is cached or committed.
//!
//! cargo run --release -p om-ui --example wave_strips -- [--out DIR] [--theme dark|daylight|FILE]
//!     [--zoom FRAMES_PER_PIXEL] [--width PX] TRACK...

mod png;

use om_analysis::{
    AudioClip, ClipSource, FileClipSource, NullAnalysisContext, WaveformBuilder, WaveformData,
};
use om_ui::theme::{parse_theme, parse_theme_file, Colour, ThemeTokens};
use om_ui::waveform::{
    hue_span, overview_column, overview_hues, strip_column, wave_colour, wave_hsl, Column,
    HUE_SPAN_COLUMNS,
};
use std::path::{Path, PathBuf};
use std::time::Instant;

const OVERVIEW_HEIGHT: usize = 80;
const STRIP_HEIGHT: usize = 140;
const KEY_HEIGHT: usize = 20;
const GAP: usize = 6;
const STRIP_SPOTS: [f64; 3] = [0.2, 0.5, 0.8];
const HUE_BINS: usize = 12;

struct Options {
    out: PathBuf,
    themes: Vec<(String, ThemeTokens)>,
    zoom: f64,
    width: usize,
    tracks: Vec<PathBuf>,
}

struct Image {
    width: usize,
    height: usize,
    rgb: Vec<u8>,
}

impl Image {
    fn new(width: usize, height: usize, background: Colour) -> Self {
        let argb = background.argb().to_be_bytes();
        let rgb = (0..width * height)
            .flat_map(|_| [argb[1], argb[2], argb[3]])
            .collect();
        Self { width, height, rgb }
    }

    fn fill(&mut self, x: usize, y: usize, h: usize, colour: Colour) {
        let argb = colour.argb().to_be_bytes();
        for row in y..(y + h).min(self.height) {
            let at = (row * self.width + x) * 3;
            self.rgb[at..at + 3].copy_from_slice(&argb[1..]);
        }
    }
}

fn builtin(name: &str) -> Option<ThemeTokens> {
    let json = match name {
        "dark" => include_str!("../../../../resources/themes/openmixer-dark.json"),
        "daylight" => include_str!("../../../../resources/themes/openmixer-daylight.json"),
        _ => return None,
    };
    parse_theme(json).ok().map(|p| p.tokens)
}

fn parse_args() -> Result<Options, String> {
    let mut o = Options {
        out: PathBuf::from("."),
        themes: Vec::new(),
        zoom: 256.0,
        width: 1600,
        tracks: Vec::new(),
    };
    let mut args = std::env::args().skip(1);
    while let Some(arg) = args.next() {
        let mut value = |name: &str| args.next().ok_or(format!("{name} needs a value"));
        match arg.as_str() {
            "--out" => o.out = PathBuf::from(value("--out")?),
            "--zoom" => o.zoom = value("--zoom")?.parse().map_err(|_| "bad --zoom")?,
            "--width" => o.width = value("--width")?.parse().map_err(|_| "bad --width")?,
            "--theme" => {
                let name = value("--theme")?;
                let tokens = builtin(&name)
                    .or_else(|| parse_theme_file(Path::new(&name)).ok().map(|p| p.tokens))
                    .ok_or(format!("unknown theme {name}"))?;
                o.themes.push((name.replace(['/', '.'], "_"), tokens));
            }
            other => o.tracks.push(PathBuf::from(other)),
        }
    }
    if o.themes.is_empty() {
        for name in ["dark", "daylight"] {
            o.themes.extend(builtin(name).map(|t| (name.to_owned(), t)));
        }
    }
    if o.tracks.is_empty() {
        return Err(
            "usage: wave_strips [--out DIR] [--theme NAME|FILE] [--zoom FPP] [--width PX] TRACK..."
                .into(),
        );
    }
    Ok(o)
}

fn draw_bar(
    image: &mut Image,
    x: usize,
    top: usize,
    height: usize,
    column: &Column,
    colour: Colour,
) {
    let mid = height as f32 / 2.0;
    let amp = (column.peak * mid * 0.9).max(1.0);
    let y = top + (mid - amp) as usize;
    image.fill(x, y, ((amp * 2.0) as usize).max(2), colour);
}

fn draw_overview(image: &mut Image, data: &WaveformData, top: usize, t: &ThemeTokens) {
    let points = &data.overview().points;
    for x in 0..image.width {
        let column = overview_column(points, x, image.width);
        let colour = wave_colour(&column, data.has_spectrum(), t);
        draw_bar(image, x, top, OVERVIEW_HEIGHT, &column, colour);
    }
}

fn draw_strip(
    image: &mut Image,
    data: &WaveformData,
    top: usize,
    centre: f64,
    o: &Options,
    t: &ThemeTokens,
) {
    let level = data.level_for(o.zoom);
    for x in 0..image.width {
        let frame = centre + (x as f64 - image.width as f64 / 2.0) * o.zoom;
        if let Some(column) = strip_column(level, frame, o.zoom) {
            let colour = wave_colour(&column, data.has_spectrum(), t);
            draw_bar(image, x, top, STRIP_HEIGHT, &column, colour);
        }
    }
}

/// The curve at full loudness, narrow spread: the palette this theme can show.
fn draw_key(image: &mut Image, top: usize, t: &ThemeTokens) {
    for x in 0..image.width {
        let column = Column {
            peak: 1.0,
            loudness: 0.5,
            centroid: x as f32 / image.width as f32,
            spread: 0.05,
        };
        image.fill(x, top, KEY_HEIGHT, wave_colour(&column, true, t));
    }
}

fn render(data: &WaveformData, o: &Options, t: &ThemeTokens) -> Image {
    let height = OVERVIEW_HEIGHT + STRIP_SPOTS.len() * (STRIP_HEIGHT + GAP) + GAP + KEY_HEIGHT;
    let mut image = Image::new(o.width, height, t.backdrop_bottom);
    draw_overview(&mut image, data, 0, t);
    let length = data.num_samples() as f64;
    for (i, spot) in STRIP_SPOTS.iter().enumerate() {
        let top = OVERVIEW_HEIGHT + GAP + i * (STRIP_HEIGHT + GAP);
        draw_strip(&mut image, data, top, length * spot, o, t);
    }
    draw_key(&mut image, height - KEY_HEIGHT, t);
    image
}

fn report(name: &str, data: &WaveformData, t: &ThemeTokens) {
    let hues = overview_hues(data, HUE_SPAN_COLUMNS, t);
    let mut bins = [0usize; HUE_BINS];
    for h in &hues {
        bins[(h.rem_euclid(360.0) / (360.0 / HUE_BINS as f32)) as usize % HUE_BINS] += 1;
    }
    let points = &data.overview().points;
    let columns: Vec<Column> = (0..HUE_SPAN_COLUMNS)
        .map(|i| overview_column(points, i, HUE_SPAN_COLUMNS))
        .collect();
    let hsl: Vec<_> = columns.iter().map(|c| wave_hsl(c, t)).collect();
    let mean = |f: fn(&om_ui::waveform::Hsl) -> f32| {
        hsl.iter().map(f).sum::<f32>() / hsl.len().max(1) as f32
    };
    println!(
        "  {name}: hue span {:.0} deg over {} audible columns; mean saturation {:.2}, lightness {:.2}",
        hue_span(&hues),
        hues.len(),
        mean(|h| h.saturation),
        mean(|h| h.lightness),
    );
    let mut spreads: Vec<f32> = columns
        .iter()
        .filter(|c| om_ui::waveform::is_audible(c, t))
        .map(|c| c.spread * om_analysis::SPREAD_MAX_OCTAVES)
        .collect();
    spreads.sort_by(f32::total_cmp);
    let pick = |p: usize| spreads.get(spreads.len() * p / 100).copied().unwrap_or(0.0);
    println!(
        "    spread octaves p10 {:.2}, p50 {:.2}, p90 {:.2}",
        pick(10),
        pick(50),
        pick(90)
    );
    println!("    hue histogram (30 deg bins from red): {bins:?}");
}

fn analyse(path: &Path) -> Result<(AudioClip, WaveformData), String> {
    let mut source = FileClipSource::new(path);
    let started = Instant::now();
    let clip = source.load(&NullAnalysisContext).ok_or("cannot decode")?;
    let decoded = started.elapsed();
    let started = Instant::now();
    let data = WaveformBuilder::default()
        .build(&clip, &NullAnalysisContext)
        .ok_or("analysis cancelled")?;
    println!(
        "{}: {:.1} s at {} Hz; decode {decoded:.1?}, analysis {:.1?}",
        path.display(),
        clip.duration_seconds(),
        clip.sample_rate,
        started.elapsed()
    );
    Ok((clip, data))
}

fn main() -> std::process::ExitCode {
    let options = match parse_args() {
        Ok(o) => o,
        Err(e) => {
            eprintln!("{e}");
            return std::process::ExitCode::from(2);
        }
    };
    let _ = std::fs::create_dir_all(&options.out);
    for track in &options.tracks {
        let (_, data) = match analyse(track) {
            Ok(r) => r,
            Err(e) => {
                eprintln!("{}: {e}", track.display());
                return std::process::ExitCode::FAILURE;
            }
        };
        let stem = track
            .file_stem()
            .map_or("track".into(), |s| s.to_string_lossy().replace(' ', "_"));
        for (name, tokens) in &options.themes {
            report(name, &data, tokens);
            let image = render(&data, &options, tokens);
            let file = options.out.join(format!("{stem}-{name}.png"));
            let written = std::fs::File::create(&file).and_then(|mut f| {
                png::write_png(&mut f, image.width as u32, image.height as u32, &image.rgb)
            });
            match written {
                Ok(()) => println!("    wrote {}", file.display()),
                Err(e) => eprintln!("    {}: {e}", file.display()),
            }
        }
    }
    std::process::ExitCode::SUCCESS
}
