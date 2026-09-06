//! Authors bounded binary TSQ1 presentation using the same public codec as playback.
use anyhow::{bail, ensure, Context, Result};
use kitu_demo_game::arena::presentation::{reference_sources, TimelineVersion};
use kitu_osc_ir::OscArg;
use kitu_tsq1::presentation::Clip;
use std::path::PathBuf;

const USAGE: &str =
    "arena-timelines [directory] [--boss-radius FLOAT] [--floor-peak-opacity FLOAT]";
struct Options {
    directory: PathBuf,
    radius: Option<f32>,
    opacity: Option<f32>,
}
fn options(args: impl IntoIterator<Item = String>) -> Result<Options> {
    let mut directory = None;
    let mut radius = None;
    let mut opacity = None;
    let mut args = args.into_iter();
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--boss-radius" | "--floor-peak-opacity" => {
                let value = args
                    .next()
                    .with_context(|| format!("missing value for {arg}; {USAGE}"))?
                    .parse::<f32>()
                    .with_context(|| format!("invalid float for {arg}"))?;
                let (target, range) = if arg == "--boss-radius" {
                    (&mut radius, 0.25..=8.0)
                } else {
                    (&mut opacity, 0.0..=1.0)
                };
                ensure!(target.is_none(), "duplicate option {arg}");
                ensure!(
                    value.is_finite() && range.contains(&value),
                    "{arg} is outside the permitted finite range"
                );
                *target = Some(value);
            }
            _ if arg.starts_with('-') => bail!("unknown option {arg}; {USAGE}"),
            _ => {
                ensure!(
                    directory.is_none(),
                    "only one output directory is accepted; {USAGE}"
                );
                directory = Some(PathBuf::from(arg));
            }
        }
    }
    ensure!(
        directory.is_some() || (radius.is_none() && opacity.is_none()),
        "custom values require an explicit output directory; {USAGE}"
    );
    Ok(Options {
        directory: directory
            .unwrap_or_else(|| PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("content/timelines")),
        radius,
        opacity,
    })
}
fn sources(options: &Options) -> Result<[Vec<u8>; 2]> {
    let [boss, floor] = reference_sources()?;
    let mut boss = Clip::decode(&boss)?;
    let mut floor = Clip::decode(&floor)?;
    if let Some(radius) = options.radius {
        for event in &mut boss.events {
            for message in &mut event.bundle.messages {
                message.args[0] = OscArg::Float(radius);
            }
        }
    }
    if let Some(opacity) = options.opacity {
        floor
            .events
            .iter_mut()
            .find(|event| event.offset_tick == 12)
            .context("reference floor keyframe missing")?
            .bundle
            .messages[0]
            .args[0] = OscArg::Float(opacity);
    }
    let sources = [boss.encode()?, floor.encode()?];
    TimelineVersion::from_sources(&sources[0], &sources[1])?;
    Ok(sources)
}
fn main() -> Result<()> {
    let options = options(std::env::args().skip(1))?;
    // Prepare both complete sources before creating or changing authoring files.
    let [boss, floor] = sources(&options)?;
    std::fs::create_dir_all(&options.directory)?;
    std::fs::write(options.directory.join("boss-telegraph.tsq"), boss)?;
    std::fs::write(options.directory.join("floor-transition.tsq"), floor)?;
    Ok(())
}
#[cfg(test)]
mod tests {
    use super::*;
    fn parse(args: &[&str]) -> Result<Options> {
        options(args.iter().map(|arg| (*arg).to_owned()))
    }
    #[test]
    fn custom_binary_authoring_validates_flags_and_preserves_terminal_fade() {
        for args in [
            vec!["--unknown"],
            vec!["--boss-radius"],
            vec!["--boss-radius", "4.5"],
            vec!["out", "--boss-radius", "NaN"],
            vec!["out", "--floor-peak-opacity", "1.1"],
            vec!["out", "--boss-radius", "4.5", "--boss-radius", "3.0"],
        ] {
            assert!(parse(&args).is_err(), "{args:?}");
        }
        let options =
            parse(&["out", "--boss-radius", "4.5", "--floor-peak-opacity", "0.9"]).unwrap();
        let [boss, floor] = sources(&options).unwrap();
        assert_eq!(
            Clip::decode(&boss).unwrap().events[0].bundle.messages[0].args[0],
            OscArg::Float(4.5)
        );
        let floor = Clip::decode(&floor).unwrap();
        assert_eq!(
            floor.events[1].bundle.messages[0].args[0],
            OscArg::Float(0.9)
        );
        assert_eq!(
            floor.events.last().unwrap().bundle.messages[0].args[0],
            OscArg::Float(0.0)
        );
    }
}
