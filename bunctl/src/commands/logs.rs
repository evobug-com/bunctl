use crate::cli::LogsArgs;

pub async fn execute(args: LogsArgs) -> anyhow::Result<()> {
    println!("Showing logs for {}", args.name);
    Ok(())
}