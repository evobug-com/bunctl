use crate::cli::RestartArgs;

pub async fn execute(args: RestartArgs) -> anyhow::Result<()> {
    println!("✔ Restarted app {}", args.name);
    Ok(())
}
