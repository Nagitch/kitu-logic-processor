//! Internal tool for creating and validating the reference Tanu content file.
use anyhow::{bail, Context, Result};
use kitu_data_tmd::tables::{DataSourceDefinition, FormulaTableCellContent, TanuDocument};
use kitu_demo_game::arena::config::ArenaConfig;

fn main() -> Result<()> {
    let args = std::env::args().skip(1).collect::<Vec<_>>();
    let [command, path] = args.as_slice() else {
        bail!("usage: arena-content create-reference|validate PATH")
    };
    match command.as_str() {
        "create-reference" => {
            let config = ArenaConfig::default();
            let mut doc = TanuDocument::read(&config.to_tmd()?)?;
            let mut sources = doc.sources()?;
            let DataSourceDefinition::FormulaTable { rows, columns, .. } =
                sources.sources.get_mut("items").context("items")?
            else {
                bail!("managed items table required")
            };
            let boss = columns.iter().position(|c| c.name == "bossDamage").unwrap();
            let damage = columns.iter().position(|c| c.name == "damage").unwrap();
            // Quick Blade is reference row 2; retain a real Formula example.
            rows[1].cells[boss].content = FormulaTableCellContent::Formula {
                expression: format!("{}2 * 1.2", (b'A' + damage as u8) as char),
            };
            doc.set_sources(sources)?;
            let bytes = doc.bytes()?;
            anyhow::ensure!(
                ArenaConfig::from_tmd(&bytes)? == config,
                "reference Formula changed evaluated defaults"
            );
            std::fs::write(path, bytes)?;
            println!("created {path}: {}", config.hash()?);
        }
        "validate" => {
            let config = ArenaConfig::from_tmd(&std::fs::read(path)?)?;
            println!(
                "{}",
                serde_json::to_string_pretty(
                    &serde_json::json!({"hash":config.hash()?,"config":config})
                )?
            );
        }
        _ => bail!("unknown content command: {command}"),
    }
    Ok(())
}
