use crate::cli::StatusArgs;

pub async fn execute(args: StatusArgs) -> anyhow::Result<()> {
    if args.json {
        println!("{{}}");
    } else {
        println!("No apps running");
    }
    Ok(())
}