mod config;
mod filter;
mod rate_limit;
mod smtp;

use clap::Parser;
use config::Config;
use rate_limit::SendRateLimiter;
use smtp::SmtpProxy;
use std::sync::Arc;

#[derive(Parser, Debug)]
#[command(author, version, about, long_about = None)]
struct Args {
    #[arg(index = 1)]
    config_path: String,

    #[arg(index = 2)]
    mode: String,
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let args = Args::parse();
    
    if args.mode != "incoming" && args.mode != "outgoing" {
        anyhow::bail!("Mode must be 'incoming' or 'outgoing'");
    }

    let config = Config::from_file(&args.config_path)?;
    let rate_limiter = Arc::new(SendRateLimiter::new());
    let proxy = SmtpProxy::new(Arc::new(config), rate_limiter, args.mode);

    proxy.run().await?;
    
    Ok(())
}
