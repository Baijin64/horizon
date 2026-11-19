//! A bug report as a local file: what the player wrote, this machine, and optionally the end of
//! the game's log, with the player's names and worlds hidden. Nothing is sent: the player reads
//! the file and attaches it to the issue by hand (docs/telemetry.md).
use crate::account::Accounts;
use crate::backend;
use crate::server_list;
use horizon_platform::system::Machine;
use horizon_telemetry::scrub::Scrubber;
use std::path::{Path, PathBuf};

/// The newest log lines kept in a report.
const LOG_LINES: usize = 300;

/// Hides the player's account names and UUIDs, their worlds' folder names and the servers they
/// play on (see [`servers`]), besides what `Scrubber` finds on its own.
fn scrubber(home: &Path, log: &str) -> Scrubber {
    let mut s = Scrubber::new();
    for account in Accounts::load().accounts {
        s = s.term(account.name).term(account.uuid);
    }
    let saves = std::fs::read_dir(home.join("saves"))
        .into_iter()
        .flatten()
        .flatten();
    for world in saves {
        s = s.term(world.file_name().to_string_lossy());
    }
    servers(s, home, log)
}

/// Also hides the hosts of the servers in `home`'s list (servers.dat), of the last one typed in
/// Direct Connection (options.txt's `lastServer`) and of those `log` shows the game connecting to
/// (its `Connecting to <host>, <port>` lines). Only names with a dot and a letter, as a server's
/// domain has: a bare word like `minecraft` would hide that word everywhere, and IP addresses
/// are already hidden. The official server is no secret and helps read the report.
fn servers(mut s: Scrubber, home: &Path, log: &str) -> Scrubber {
    let listed = server_list::List::load(home)
        .servers()
        .into_iter()
        .map(|server| server.ip);
    let last = std::fs::read_to_string(home.join("options.txt"))
        .unwrap_or_default()
        .lines()
        .find_map(|l| l.strip_prefix("lastServer:").map(str::to_owned));
    let connected = log.lines().filter_map(|line| {
        let (host, port) = line.split_once("Connecting to ")?.1.rsplit_once(", ")?;
        port.trim().parse::<u16>().ok().map(|_| host.to_owned())
    });
    let official: Vec<String> = std::iter::once(server_list::OFFICIAL_ADDRESS)
        .chain(server_list::OFFICIAL_PREVIOUS.iter().copied())
        .map(|a| host(a).to_ascii_lowercase())
        .collect();
    for address in listed.chain(last).chain(connected) {
        let host = host(&address);
        if host.contains('.')
            && host.contains(|c: char| c.is_ascii_alphabetic())
            && !official.contains(&host.to_ascii_lowercase())
        {
            s = s.term(host);
        }
    }
    s
}

/// The host of a server address: without its port, and without the brackets of an IPv6 one.
fn host(address: &str) -> &str {
    let address = address.trim();
    if let Some(v6) = address.strip_prefix('[') {
        return v6.split(']').next().unwrap_or(v6);
    }
    match address.rsplit_once(':') {
        Some((host, port)) if !host.contains(':') && port.parse::<u16>().is_ok() => host,
        _ => address,
    }
}

/// Writes `bug-reports/bug-report-<time>.txt` in the game folder and returns its path.
pub fn write(text: &str, with_log: bool) -> Result<PathBuf, String> {
    let home = backend::game_home();
    let log = match with_log {
        true => Some(backend::read_logs(LOG_LINES, false)?.1),
        false => None,
    };
    let scrubber = scrubber(&home, log.as_deref().unwrap_or_default());
    let machine = Machine::detect_in(Some(&home));
    let mut report = format!(
        "{}\n\nHorizon launcher {}\n{} {} on {}, {} ({} cores, {} threads, {} GB)\n",
        scrubber.scrub(text.trim()),
        backend::VERSION,
        machine.os,
        machine.os_version,
        machine.arch,
        machine.cpu,
        machine.cores,
        machine.threads,
        machine.ram_gb,
    );
    if let Some(log) = log {
        let kept: Vec<&str> = log.lines().filter(|l| !l.contains("[CHAT]")).collect();
        report.push_str("\n--- end of the game's log ---\n");
        report.push_str(&scrubber.scrub(&kept.join("\n")));
        report.push('\n');
    }
    let dir = home.join("bug-reports");
    std::fs::create_dir_all(&dir).map_err(|e| format!("{}: {e}", dir.display()))?;
    let path = dir.join(format!(
        "bug-report-{}.txt",
        chrono::Local::now().format("%Y-%m-%d_%H.%M.%S")
    ));
    std::fs::write(&path, report).map_err(|e| format!("{}: {e}", path.display()))?;
    Ok(path)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_servers_the_player_plays_on_are_hidden_and_ordinary_words_stay() {
        let home = std::env::temp_dir().join(format!(
            "horizon-launcher-bug-report-servers-{}",
            std::process::id()
        ));
        let _ = std::fs::remove_dir_all(&home);
        std::fs::create_dir_all(&home).unwrap();
        let mut list = server_list::List::load(&home);
        list.add("Friends", "mc.friends-castle.org:25570").unwrap();
        list.add("Home", "minecraft").unwrap();
        list.save(&home).unwrap();
        std::fs::write(
            home.join("options.txt"),
            "lang:en_us\nlastServer:typed.example.net\n",
        )
        .unwrap();
        let official = host(server_list::OFFICIAL_ADDRESS).to_owned();
        let log = format!(
            "[12:00:00] [Server Connector #1/INFO]: Connecting to Play.Secret-Host.net, 25565\n\
             [12:00:01] [Render thread/INFO]: Joining Play.Secret-Host.net as a 1.21 server\n\
             [12:00:02] [Server Connector #2/INFO]: Connecting to {official}, 25565\n\
             [12:01:00] [Render thread/INFO]: Left mc.friends-castle.org:25570 and typed.example.net\n\
             [12:02:00] [Render thread/INFO]: Loaded minecraft textures for the server list"
        );

        let scrubbed = servers(Scrubber::bare(), &home, &log).scrub(&log);
        assert_eq!(
            scrubbed,
            format!(
                "[12:00:00] [Server Connector #1/INFO]: Connecting to <private>, 25565\n\
                 [12:00:01] [Render thread/INFO]: Joining <private> as a 1.21 server\n\
                 [12:00:02] [Server Connector #2/INFO]: Connecting to {official}, 25565\n\
                 [12:01:00] [Render thread/INFO]: Left <private>:25570 and <private>\n\
                 [12:02:00] [Render thread/INFO]: Loaded minecraft textures for the server list"
            )
        );
        std::fs::remove_dir_all(&home).unwrap();
    }
}
