//! Authoring tool for real Tanu/SQLite sources and fixed-order source plans.
use anyhow::{bail, Context, Result};
use kitu_data_tmd::tables::{DataSourceDefinition, FormulaTableCellContent, TanuDocument};
use kitu_demo_game::arena::config::{create_source_plan, load_content, write_sqlite, ArenaConfig};
use std::path::Path;

fn main() -> Result<()> {
    let args = std::env::args().skip(1).collect::<Vec<_>>();
    let [command, path] = args.as_slice() else {
        bail!("usage: arena-content create-reference|create-sqlite|create-plan|validate PATH")
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
            let loaded = load_content(Path::new(path))?;
            println!("{}", serde_json::to_string_pretty(&loaded)?);
        }
        "create-sqlite" => {
            write_sqlite(Path::new(path), &ArenaConfig::default().to_tables()?)?;
            println!("created {path}");
        }
        "create-plan" => {
            create_source_plan(Path::new(path))?;
            println!("created {path} and its four editable sources");
        }
        _ => bail!("unknown content command: {command}"),
    }
    Ok(())
}
