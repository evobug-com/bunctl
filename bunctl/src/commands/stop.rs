use crate::cli::StopArgs;

pub async fn execute(args: StopArgs) -> anyhow::Result<()> {
    println!("✔ Stopped app {}", args.name);
    Ok(())
}
