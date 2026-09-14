use ribosome_import::{Error, Limits, Result, reader::prepare};
use std::path::Path;

fn main() {
    if let Err(error) = command() {
        eprintln!("{}", serde_json::to_string(&error).unwrap());
        std::process::exit(1);
    }
}
fn command() -> Result<()> {
    let args = std::env::args().skip(1).collect::<Vec<_>>();
    match args.first().map(String::as_str) {
        Some("prepare") if (3..=4).contains(&args.len()) => {
            let limits: Limits = if let Some(path) = args.get(3) {
                serde_json::from_slice(&std::fs::read(path)?)?
            } else {
                Limits::default()
            };
            let result = prepare(Path::new(&args[1]), Path::new(&args[2]), &limits)?;
            println!("{}", serde_json::to_string_pretty(&result)?);
        }
        Some("assign") if args.len() == 4 => {
            let config = serde_json::from_slice(&std::fs::read(&args[2])?)?;
            let result =
                ribosome_import::study::assign(Path::new(&args[1]), &config, Path::new(&args[3]))?;
            println!("{}", serde_json::to_string_pretty(&result)?);
        }
        Some("withdraw") if args.len() == 5 => {
            let config = serde_json::from_slice(&std::fs::read(&args[2])?)?;
            let source =
                ribosome_import::study::withdraw(Path::new(&args[1]), &config, &args[3], &args[4])?;
            println!("{}", serde_json::json!({"withdrawn_source":source}));
        }
        None | Some("--help") => println!(
            "ribosome-import prepare PROFILE.yaml OUTPUT [LIMITS.json]\nribosome-import assign STUDY.json OWNER_CONFIG.json OUTPUT\nribosome-import withdraw STUDY.json OWNER_CONFIG.json COHORT EPISODE\n\nPreparation writes owner-only source files and episodes. Assignment publishes selected evidence through the existing store and corpus. Withdrawal removes generated workspace projections and withdraws dependent records; owner acquisition files remain.\nBuild with --features remote for pinned HF and Parquet inputs."
        ),
        _ => {
            return Err(Error::invalid(
                "unknown command; use ribosome-import --help",
            ));
        }
    }
    Ok(())
}
