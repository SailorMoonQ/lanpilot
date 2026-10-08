//! `lanpilot-agent`: the headless LanPilot agent (M1).

use clap::{Parser, Subcommand};
use lanpilot_agent::AgentError;
use lanpilot_agent::config::Config;
use lanpilot_agent::devices::DeviceStore;
use lanpilot_agent::pairing::{lan_ipv4_addrs, render_qr};
use lanpilot_agent::paths::Paths;
use lanpilot_agent::secret::{clear_password, store_password};
use lanpilot_agent::server::Agent;
use lanpilot_core::discovery::Advertiser;
use lanpilot_core::pairing::tokens::TOKEN_TTL;
use lanpilot_input::InputBackend;
use lanpilot_input::logging::LoggingBackend;
use std::collections::BTreeSet;
use std::net::{IpAddr, SocketAddr};
use std::path::PathBuf;
use std::time::Duration;

#[derive(Parser)]
#[command(name = "lanpilot-agent", version, about = "LanPilot PC agent")]
struct Cli {
    /// Keep config and state under this directory instead of the user profile.
    #[arg(long, global = true)]
    home: Option<PathBuf>,
    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    /// Run the agent.
    Run {
        /// Show a pairing QR code (refreshed every 2 minutes).
        #[arg(long)]
        pair: bool,
        /// Log input instead of injecting it (for phone app development;
        /// set RUST_LOG=lanpilot_input=debug to see each call).
        #[arg(long)]
        mock_input: bool,
    },
    /// Manage the pairing password.
    Password {
        #[command(subcommand)]
        action: PasswordAction,
    },
    /// Manage paired devices.
    Devices {
        #[command(subcommand)]
        action: DevicesAction,
    },
    /// Print where config and state are stored.
    Paths,
}

#[derive(Subcommand)]
enum PasswordAction {
    /// Set the password and enable password pairing.
    Set,
    /// Remove the password and disable password pairing.
    Clear,
}

#[derive(Subcommand)]
enum DevicesAction {
    List,
    /// Remove a device by its short ID (see `devices list`).
    Remove {
        short_id: String,
    },
}

fn main() {
    let cli = Cli::parse();
    if let Err(e) = real_main(cli) {
        eprintln!("error: {e}");
        std::process::exit(1);
    }
}

fn real_main(cli: Cli) -> Result<(), AgentError> {
    let paths = match &cli.home {
        Some(root) => Paths::under(root),
        None => Paths::default_for_user()?,
    };
    match cli.command {
        Command::Run { pair, mock_input } => run(&paths, pair, mock_input),
        Command::Password {
            action: PasswordAction::Set,
        } => {
            let pw = rpassword::prompt_password("New pairing password: ")?;
            let again = rpassword::prompt_password("Repeat: ")?;
            if pw != again {
                return Err(AgentError::Config("passwords do not match".into()));
            }
            store_password(&paths.password_file(), &pw)?;
            let mut config = Config::load_or_default(&paths.config_file)?;
            config.pairing.password_enabled = true;
            config.save(&paths.config_file)?;
            println!("Password pairing enabled. Restart a running agent to apply.");
            Ok(())
        }
        Command::Password {
            action: PasswordAction::Clear,
        } => {
            clear_password(&paths.password_file())?;
            let mut config = Config::load_or_default(&paths.config_file)?;
            config.pairing.password_enabled = false;
            config.save(&paths.config_file)?;
            println!("Password pairing disabled. Restart a running agent to apply.");
            Ok(())
        }
        Command::Devices {
            action: DevicesAction::List,
        } => {
            let store = DeviceStore::open(paths.devices_file())?;
            let devices = store.list();
            if devices.is_empty() {
                println!("No paired devices.");
            }
            for d in devices {
                println!("{}  {:<24} {:?}", d.public_key.short_id(), d.name, d.os);
            }
            Ok(())
        }
        Command::Devices {
            action: DevicesAction::Remove { short_id },
        } => {
            let store = DeviceStore::open(paths.devices_file())?;
            match store.remove_by_short_id(&short_id)? {
                Some(d) => println!("Removed {} ({}).", d.name, short_id),
                None => println!("No device with short ID {short_id}."),
            }
            Ok(())
        }
        Command::Paths => {
            println!("config: {}", paths.config_file.display());
            println!("state:  {}", paths.state_dir.display());
            Ok(())
        }
    }
}

fn run(paths: &Paths, pair: bool, mock_input: bool) -> Result<(), AgentError> {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("info")),
        )
        .init();
    let input: Box<dyn InputBackend> = if mock_input {
        tracing::warn!("mock input: input is logged, not injected");
        Box::new(LoggingBackend::new())
    } else {
        lanpilot_input::open_default().map_err(|e| AgentError::Core(e.to_string()))?
    };
    let runtime = tokio::runtime::Runtime::new()?;
    runtime.block_on(async {
        let agent = Agent::new(paths, input)?;
        let port = agent.config().general.port;
        let endpoint = agent.bind(SocketAddr::from(([0, 0, 0, 0], port)))?;
        let mut addrs = lan_ipv4_addrs();
        let mut advertiser = start_advertiser(&agent, port, addrs.clone());
        tracing::info!(
            "LanPilot agent \"{}\" ({}) listening on UDP {port}",
            agent.config().general.name,
            agent.public_key().short_id()
        );
        if pair {
            let agent = agent.clone();
            tokio::spawn(async move {
                loop {
                    let invite = agent.invite(port, lan_ipv4_addrs());
                    let uri = invite.to_uri();
                    println!(
                        "\nScan to pair (valid {} s):\n{}\n{uri}\n",
                        TOKEN_TTL.as_secs(),
                        render_qr(&uri)
                    );
                    tokio::time::sleep(TOKEN_TTL).await;
                }
            });
        }
        let serving = tokio::spawn(agent.clone().serve(endpoint.clone()));
        let mut refresh = tokio::time::interval(ADVERTISE_REFRESH);
        refresh.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);
        refresh.tick().await;
        let ctrl_c = tokio::signal::ctrl_c();
        tokio::pin!(ctrl_c);
        loop {
            tokio::select! {
                r = &mut ctrl_c => {
                    r?;
                    break;
                }
                _ = refresh.tick() => {
                    let latest = lan_ipv4_addrs();
                    if addrs_changed(&addrs, &latest) {
                        tracing::info!(
                            "LAN addresses changed from {addrs:?} to {latest:?}, re-advertising"
                        );
                        if let Some(old) = advertiser.take() {
                            old.stop().await;
                        }
                        advertiser = start_advertiser(&agent, port, latest.clone());
                        addrs = latest;
                    }
                }
            }
        }
        tracing::info!("shutting down");
        if let Some(advertiser) = advertiser.take() {
            advertiser.stop().await;
        }
        agent.shutdown(&endpoint).await;
        serving.abort();
        Ok::<(), AgentError>(())
    })
}

/// How often the agent checks whether its LAN addresses changed.
const ADVERTISE_REFRESH: Duration = Duration::from_secs(30);

/// Starts mDNS on `addrs`; a failure is logged, not fatal.
fn start_advertiser(agent: &Agent, port: u16, addrs: Vec<IpAddr>) -> Option<Advertiser> {
    tracing::info!("mDNS advertising on {addrs:?}");
    match agent.advertise(port, addrs) {
        Ok(a) => Some(a),
        Err(e) => {
            tracing::warn!("mDNS advertising failed, phones must use the QR code or the IP: {e}");
            None
        }
    }
}

/// Whether two address lists differ as sets.
fn addrs_changed(current: &[IpAddr], latest: &[IpAddr]) -> bool {
    let set = |a: &[IpAddr]| a.iter().copied().collect::<BTreeSet<IpAddr>>();
    set(current) != set(latest)
}

#[cfg(test)]
mod tests {
    use super::{Cli, Command, addrs_changed};
    use clap::{CommandFactory, Parser};
    use std::net::IpAddr;

    #[test]
    fn addrs_change_is_a_set_comparison() {
        let a: IpAddr = "192.168.50.203".parse().unwrap();
        let b: IpAddr = "10.0.0.7".parse().unwrap();
        assert!(!addrs_changed(&[a, b], &[b, a]));
        assert!(!addrs_changed(&[a], &[a, a]));
        assert!(addrs_changed(&[a], &[a, b]));
        assert!(addrs_changed(&[a], &[]));
        assert!(addrs_changed(&[], &[b]));
        assert!(!addrs_changed(&[], &[]));
    }

    #[test]
    fn cli_definition_is_valid() {
        Cli::command().debug_assert();
    }

    #[test]
    fn run_accepts_mock_input() {
        let cli = Cli::try_parse_from(["lanpilot-agent", "run", "--mock-input"]).unwrap();
        assert!(matches!(
            cli.command,
            Command::Run {
                mock_input: true,
                pair: false
            }
        ));
        let cli = Cli::try_parse_from(["lanpilot-agent", "run"]).unwrap();
        assert!(matches!(
            cli.command,
            Command::Run {
                mock_input: false,
                ..
            }
        ));
    }
}
