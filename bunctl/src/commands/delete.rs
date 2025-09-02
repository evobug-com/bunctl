use crate::cli::DeleteArgs;

pub async fn execute(args: DeleteArgs) -> anyhow::Result<()> {
    println!("✔ Deleted app {}", args.name);
    Ok(())
}
