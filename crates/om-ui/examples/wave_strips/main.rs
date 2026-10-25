//! Tuning tool for the waveform: renders the overview and three zoomed strips to PNG, per theme and
//! style, with the same pure functions the painter uses (palette, column shapes, bucket keys,
//! `shape_px`) and a software fill in place of GPUI paths. Nothing is played, cached or committed.
//!
//! cargo run --release -p om-ui --example wave_strips -- [--out DIR] [--theme dark|daylight|FILE]
//!     [--style colour|three-band|mono|colour-blind] [--zoom FRAMES_PER_PIXEL] [--width PX]
//!     (--synthetic | TRACK...)
//!
//! `--synthetic` analyses `om_analysis::demo_loop` (no input needed). Without `--theme` both
//! built-in themes are drawn; without `--style` the Colour style. The painter reads the style from
//! the `OPENMIXER_WAVE_STYLE` environment variable the same way.

mod png;

use om_analysis::{
    demo_loop, AudioClip, ClipSource, FileClipSource, NullAnalysisContext, WaveformBuilder,
    WaveformData,
};
use om_ui::style::over;
use om_ui::theme::{parse_theme, parse_theme_file, Colour, ThemeTokens};
use om_ui::waveform::{
    column_shapes, overview_column, reference_peak, shape_px, strip_column, strip_well, weights,
    Band, ColumnShape, Palette, WaveStyle,
};
use std::path::{Path, PathBuf};
use std::time::Instant;

const OVERVIEW_HEIGHT: usize = 48;
const STRIP_HEIGHT: usize = 140;
const KEY_HEIGHT: usize = 20;
const GAP: usize = 6;
const STRIP_SPOTS: [f64; 3] = [0.2, 0.5, 0.8];
const SYNTHETIC_SECONDS: f64 = 64.0;
const SYNTHETIC_BPM: f64 = 124.0;
const RATE: f64 = 44_100.0;

struct Options {
    out: PathBuf,
    themes: Vec<(String, ThemeTokens)>,
    styles: Vec<WaveStyle>,
    zoom: f64,
    width: usize,
    synthetic: bool,
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

    /// Source-over of `colour` (its alpha) onto the pixels `x`, rows `y..y + h`.
    fn blend(&mut self, x: usize, y: usize, h: usize, colour: Colour) {
        if x >= self.width {
            return;
        }
        let a = colour.alpha();
        let argb = colour.argb().to_be_bytes();
        for row in y..(y + h).min(self.height) {
            let at = (row * self.width + x) * 3;
            for c in 0..3 {
                let below = f32::from(self.rgb[at + c]);
                self.rgb[at + c] = (f32::from(argb[c + 1]) * a + below * (1.0 - a)).round() as u8;
            }
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

const USAGE: &str =
    "usage: wave_strips [--out DIR] [--theme NAME|FILE] [--style STYLE] [--zoom FPP] \
[--width PX] (--synthetic | TRACK...)";

fn parse_args() -> Result<Options, String> {
    let mut o = Options {
        out: PathBuf::from("."),
        themes: Vec::new(),
        styles: Vec::new(),
        zoom: 256.0,
        width: 1600,
        synthetic: false,
        tracks: Vec::new(),
    };
    let mut args = std::env::args().skip(1);
    while let Some(arg) = args.next() {
        let mut value = |name: &str| args.next().ok_or(format!("{name} needs a value"));
        match arg.as_str() {
            "--out" => o.out = PathBuf::from(value("--out")?),
            "--zoom" => o.zoom = value("--zoom")?.parse().map_err(|_| "bad --zoom")?,
            "--width" => o.width = value("--width")?.parse().map_err(|_| "bad --width")?,
            "--synthetic" => o.synthetic = true,
            "--style" => {
                let name = value("--style")?;
                o.styles
                    .push(WaveStyle::from_name(&name).ok_or(format!("unknown style {name}"))?);
            }
            "--theme" => {
                let name = value("--theme")?;
                let tokens = builtin(&name)
                    .or_else(|| parse_theme_file(Path::new(&name)).ok().map(|p| p.tokens))
                    .ok_or(format!("unknown theme {name}"))?;
                o.themes.push((name.replace(['/', '.'], "_"), tokens));
            }
            "--help" | "-h" => return Err(USAGE.into()),
            other => o.tracks.push(PathBuf::from(other)),
        }
    }
    if o.themes.is_empty() {
        for name in ["dark", "daylight"] {
            o.themes.extend(builtin(name).map(|t| (name.to_owned(), t)));
        }
    }
    if o.styles.is_empty() {
        o.styles.push(WaveStyle::Colour);
    }
    if o.tracks.is_empty() && !o.synthetic {
        return Err(USAGE.into());
    }
    Ok(o)
}

/// Draws one column the way the painter does: the peak silhouette at low alpha, then the body in
/// its bucket's colour; the three-band style draws three nested bars instead.
fn draw_column(
    image: &mut Image,
    x: usize,
    top: usize,
    height: usize,
    shape: &ColumnShape,
    palette: &Palette,
    spectral: bool,
) {
    let mid = top as f32 + height as f32 / 2.0;
    let bar = |image: &mut Image, half: f32, colour: Colour| {
        if half > 0.0 {
            let from = (mid - half).max(top as f32).round() as usize;
            let to = (mid + half).min((top + height) as f32).round() as usize;
            image.blend(x, from, to.saturating_sub(from).max(1), colour);
        }
    };
    let h = height as f32;
    if !spectral {
        bar(image, shape_px(shape.silhouette, h), palette.silhouette);
    } else if palette.style == WaveStyle::ThreeBand {
        for (band, colour) in Band::ALL.into_iter().zip(palette.bands()) {
            bar(image, shape_px(shape.bands.get(band), h), colour);
        }
    } else {
        bar(
            image,
            shape_px(shape.body, h),
            palette.colour(shape.weights),
        );
    }
}

fn shapes_for(
    data: &WaveformData,
    reference: f32,
    count: usize,
    pick: impl Fn(usize) -> Option<om_ui::waveform::Column>,
) -> Vec<ColumnShape> {
    let columns: Vec<_> = (0..count).map(pick).collect();
    column_shapes(&columns, reference, data.has_spectrum())
}

fn draw_shapes(
    image: &mut Image,
    top: usize,
    height: usize,
    shapes: &[ColumnShape],
    palette: &Palette,
    spectral: bool,
) {
    for (x, shape) in shapes.iter().enumerate() {
        draw_column(image, x, top, height, shape, palette, spectral);
    }
}

/// Beat and bar lines over a strip, in the grid colour at the painter's alphas (white on dark).
fn draw_grid(image: &mut Image, top: usize, centre: f64, zoom: f64, bpm: f64, grid: Colour) {
    let beat = RATE * 60.0 / bpm;
    let first = ((centre - image.width as f64 / 2.0 * zoom) / beat)
        .floor()
        .max(0.0) as i64;
    let last = ((centre + image.width as f64 / 2.0 * zoom) / beat).ceil() as i64;
    for n in first..=last {
        let x = image.width as f64 / 2.0 + (n as f64 * beat - centre) / zoom;
        if x < 0.0 || x >= image.width as f64 {
            continue;
        }
        let bar = n % 4 == 0;
        let (width, alpha) = if bar { (2, 0.70) } else { (1, 0.25) };
        for dx in 0..width {
            image.blend(x as usize + dx, top, STRIP_HEIGHT, grid.with_alpha(alpha));
        }
    }
}

fn render(
    data: &WaveformData,
    o: &Options,
    well: Colour,
    palette: &Palette,
    grid: Option<f64>,
) -> Image {
    let height = OVERVIEW_HEIGHT + STRIP_SPOTS.len() * (STRIP_HEIGHT + GAP) + GAP + KEY_HEIGHT;
    let mut image = Image::new(o.width, height, well);
    let points = &data.overview().points;
    let reference = reference_peak(points);
    let spectral = data.has_spectrum();

    let overview = shapes_for(data, reference, o.width, |i| {
        Some(overview_column(points, i, o.width))
    });
    draw_shapes(&mut image, 0, OVERVIEW_HEIGHT, &overview, palette, spectral);

    let length = data.num_samples() as f64;
    let level = data.level_for(o.zoom);
    for (i, spot) in STRIP_SPOTS.iter().enumerate() {
        let top = OVERVIEW_HEIGHT + GAP + i * (STRIP_HEIGHT + GAP);
        let centre = length * spot;
        let shapes = shapes_for(data, reference, o.width, |x| {
            let frame = centre + (x as f64 - o.width as f64 / 2.0) * o.zoom;
            strip_column(level, frame, o.zoom)
        });
        draw_shapes(&mut image, top, STRIP_HEIGHT, &shapes, palette, spectral);
        if let Some(bpm) = grid {
            let grid_colour = Colour::from_argb(0xFFFF_FFFF);
            draw_grid(&mut image, top, centre, o.zoom, bpm, grid_colour);
        }
    }

    // Key row: the three band colours pure, then a ramp from the first band to the second to the
    // third through every bucket weight.
    let key_top = height - KEY_HEIGHT;
    let third = o.width / 3;
    for (i, colour) in palette.bands().into_iter().enumerate() {
        for x in i * third..(i + 1) * third - 2 {
            image.blend(x, key_top, KEY_HEIGHT, colour);
        }
    }
    image
}

fn distance(a: Colour, b: Colour) -> f32 {
    let rgb = |c: Colour| [16, 8, 0].map(|sh| ((c.argb() >> sh) & 0xFF) as f32 / 255.0);
    let (a, b) = (rgb(a), rgb(b));
    (0..3).map(|k| (a[k] - b[k]).powi(2)).sum::<f32>().sqrt()
}

/// The gates on this track, as numbers: overview columns at full height, columns that read as
/// washed out (saturation under 0.25), and the colour difference between adjacent columns in
/// steady sections of the overview (both columns tall, heights within 0.05).
fn report(name: &str, data: &WaveformData, width: usize, palette: &Palette) {
    let points = &data.overview().points;
    let reference = reference_peak(points);
    let columns: Vec<_> = (0..width)
        .map(|i| overview_column(points, i, width))
        .collect();
    let shapes = shapes_for(data, reference, width, |i| Some(columns[i]));
    let audible: Vec<usize> = (0..width).filter(|&i| columns[i].peak > 0.02).collect();
    let n = audible.len().max(1) as f32;
    let full = audible
        .iter()
        .filter(|&&i| shapes[i].silhouette >= 1.0)
        .count();
    let washed = audible
        .iter()
        .filter(|&&i| {
            let c = palette.colour(weights(&columns[i]));
            let rgb = [16, 8, 0].map(|sh| ((c.argb() >> sh) & 0xFF) as f32 / 255.0);
            let max = rgb.iter().cloned().fold(0.0, f32::max);
            let min = rgb.iter().cloned().fold(1.0, f32::min);
            max > 0.0 && (max - min) / max < 0.25
        })
        .count();
    let mut steps: Vec<f32> = shapes
        .windows(2)
        .filter(|w| {
            w[0].silhouette >= 0.3
                && w[1].silhouette >= 0.3
                && (w[0].silhouette - w[1].silhouette).abs() <= 0.05
        })
        .map(|w| distance(palette.colour(w[0].weights), palette.colour(w[1].weights)))
        .collect();
    steps.sort_by(f32::total_cmp);
    let at = |p: usize| {
        steps
            .get(steps.len().saturating_sub(1) * p / 100)
            .copied()
            .unwrap_or(0.0)
    };
    println!(
        "  {name}: {} audible overview columns; full height {:.1}%, washed out {:.1}%, adjacent colour step median {:.3} p95 {:.3} ({} steady pairs)",
        audible.len(),
        100.0 * full as f32 / n,
        100.0 * washed as f32 / n,
        at(50),
        at(95),
        steps.len()
    );
}

fn analyse(clip: &AudioClip, label: &str) -> Result<WaveformData, String> {
    let started = Instant::now();
    let data = WaveformBuilder::default()
        .build(clip, &NullAnalysisContext)
        .ok_or("analysis cancelled")?;
    println!(
        "{label}: {:.1} s at {} Hz; analysis {:.1?}",
        clip.duration_seconds(),
        clip.sample_rate,
        started.elapsed()
    );
    Ok(data)
}

fn load(path: &Path) -> Result<AudioClip, String> {
    FileClipSource::new(path)
        .load(&NullAnalysisContext)
        .ok_or_else(|| "cannot decode".to_owned())
}

fn write(options: &Options, stem: &str, data: &WaveformData, grid: Option<f64>) {
    for (name, tokens) in &options.themes {
        let well = strip_well(tokens);
        // The strip is drawn over an opaque colour; PNG has no alpha here.
        let well = over(well, Colour::from_argb(0xFF00_0000));
        for &style in &options.styles {
            let palette = Palette::new(style, well);
            println!("  {name} / {}", style.name());
            report(
                &format!("{name}-{}", style.name()),
                data,
                options.width,
                &palette,
            );
            let image = render(data, options, well, &palette, grid);
            let file = options
                .out
                .join(format!("{stem}-{name}-{}.png", style.name()));
            let written = std::fs::File::create(&file).and_then(|mut f| {
                png::write_png(&mut f, image.width as u32, image.height as u32, &image.rgb)
            });
            match written {
                Ok(()) => println!("    wrote {}", file.display()),
                Err(e) => eprintln!("    {}: {e}", file.display()),
            }
        }
    }
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
    if options.synthetic {
        let clip = AudioClip::new(demo_loop(SYNTHETIC_SECONDS, SYNTHETIC_BPM, RATE), RATE);
        match analyse(&clip, "synthetic demo loop") {
            Ok(data) => write(&options, "synthetic", &data, Some(SYNTHETIC_BPM)),
            Err(e) => {
                eprintln!("synthetic: {e}");
                return std::process::ExitCode::FAILURE;
            }
        }
    }
    for track in &options.tracks {
        let result = load(track).and_then(|clip| analyse(&clip, &track.display().to_string()));
        let data = match result {
            Ok(d) => d,
            Err(e) => {
                eprintln!("{}: {e}", track.display());
                return std::process::ExitCode::FAILURE;
            }
        };
        let stem = track
            .file_stem()
            .map_or("track".into(), |s| s.to_string_lossy().replace(' ', "_"));
        write(&options, &stem, &data, None);
    }
    std::process::ExitCode::SUCCESS
}
