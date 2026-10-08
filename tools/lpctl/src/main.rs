use clap::{Parser, Subcommand};
use lanpilot_core::discovery::{Browser, DiscoveryEvent};
use lpctl::store::ClientStore;
use lpctl::{Action, LpctlError, pair_password, pair_uri, parse_button, parse_media, perform};
use std::net::SocketAddr;
use std::path::PathBuf;
use std::time::Duration;

#[derive(Parser)]
#[command(name = "lpctl", about = "LanPilot development client")]
struct Cli {
    /// Directory holding lpctl's identity and paired servers.
    #[arg(long, global = true)]
    home: Option<PathBuf>,
    /// Connect to this address instead of the stored ones.
    #[arg(long, global = true)]
    addr: Option<SocketAddr>,
    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    /// Pair using the lanpilot://pair URI printed by `lanpilot-agent run --pair`.
    Pair {
        uri: String,
    },
    /// Pair with the agent's pairing password.
    PairPassword {
        addr: SocketAddr,
    },
    /// List agents announcing themselves via mDNS.
    Discover {
        #[arg(long, default_value_t = 5)]
        secs: u64,
    },
    /// List paired agents.
    List,
    Move {
        server: String,
        dx: f32,
        dy: f32,
        #[arg(long, default_value_t = 20)]
        steps: u32,
    },
    Click {
        server: String,
        #[arg(default_value = "left")]
        button: String,
    },
    Scroll {
        server: String,
        notches: f32,
    },
    /// Press a chord, e.g. `lpctl key desk ctrl c`.
    Key {
        server: String,
        #[arg(required = true)]
        names: Vec<String>,
    },
    Media {
        server: String,
        action: String,
    },
    Text {
        server: String,
        text: String,
    },
    Unpair {
        server: String,
    },
    /// Draw a 200 px square with the pointer.
    Square {
        server: String,
    },
}

#[tokio::main]
async fn main() {
    if let Err(e) = real_main(Cli::parse()).await {
        eprintln!("error: {e}");
        std::process::exit(1);
    }
}

async fn real_main(cli: Cli) -> Result<(), LpctlError> {
    let home = match cli.home {
        Some(h) => h,
        None => dirs::config_dir()
            .ok_or_else(|| LpctlError::Usage("no config directory".into()))?
            .join("lanpilot-ctl"),
    };
    let mut store = ClientStore::open(home)?;
    let usage = |m: String| LpctlError::Usage(m);
    let action_for = |server: String, action: Action| (server, action);
    let (selector, action) = match cli.command {
        Command::Pair { uri } => {
            let s = pair_uri(&mut store, &uri, "lpctl").await?;
            println!("Paired with {} ({}).", s.name, s.public_key.short_id());
            return Ok(());
        }
        Command::PairPassword { addr } => {
            let pw = rpassword::prompt_password("Pairing password: ")?;
            let s = pair_password(&mut store, addr, &pw, "lpctl").await?;
            println!("Paired with {} ({}).", s.name, s.public_key.short_id());
            return Ok(());
        }
        Command::Discover { secs } => {
            let mut browser = Browser::start().map_err(|e| usage(e.to_string()))?;
            let deadline = tokio::time::sleep(Duration::from_secs(secs));
            tokio::pin!(deadline);
            loop {
                tokio::select! {
                    _ = &mut deadline => return Ok(()),
                    ev = browser.next() => match ev {
                        Some(DiscoveryEvent::Found(d)) => println!("{}  {:<24} {:?}  {:?}:{}", d.short_id, d.name, d.os, d.addrs, d.port),
                        Some(DiscoveryEvent::Lost { short_id }) => println!("{short_id}  (gone)"),
                        None => return Ok(()),
                    },
                }
            }
        }
        Command::List => {
            for s in store.servers() {
                println!(
                    "{}  {:<24} {:?}:{}",
                    s.public_key.short_id(),
                    s.name,
                    s.addrs,
                    s.port
                );
            }
            return Ok(());
        }
        Command::Move {
            server,
            dx,
            dy,
            steps,
        } => action_for(server, Action::Move { dx, dy, steps }),
        Command::Click { server, button } => {
            let b =
                parse_button(&button).ok_or_else(|| usage(format!("unknown button '{button}'")))?;
            action_for(server, Action::Click(b))
        }
        Command::Scroll { server, notches } => action_for(server, Action::Scroll(notches)),
        Command::Key { server, names } => {
            let usages = names
                .iter()
                .map(|n| {
                    lanpilot_input::usage_from_name(n)
                        .map(|u| u.0)
                        .ok_or_else(|| usage(format!("unknown key '{n}'")))
                })
                .collect::<Result<Vec<u32>, _>>()?;
            action_for(server, Action::Keys(usages))
        }
        Command::Media { server, action } => {
            let a = parse_media(&action)
                .ok_or_else(|| usage(format!("unknown media action '{action}'")))?;
            action_for(server, Action::Media(a))
        }
        Command::Text { server, text } => action_for(server, Action::Text(text)),
        Command::Unpair { server } => action_for(server, Action::Unpair),
        Command::Square { server } => action_for(server, Action::Square),
    };
    let server = store.find(&selector)?.clone();
    perform(&mut store, &server, cli.addr, action).await
}
