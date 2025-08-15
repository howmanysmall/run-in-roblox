mod message_receiver;
mod place_runner;
mod plugin;

use std::{path::PathBuf, process, sync::mpsc, thread};

use anyhow::anyhow;
use clap::Parser;
use colored::Colorize;
use fs_err as fs;
use tempfile::tempdir;

use crate::{
    message_receiver::{OutputLevel, RobloxMessage},
    place_runner::PlaceRunner,
};

#[derive(Debug, Parser)]
#[command(about = "Run stuff inside Roblox Studio", long_about = None)]
struct Options {
    /// A path to the place file to open in Roblox Studio. If not specified, an
    /// empty place file is used.
    #[arg(long = "place")]
    place_path: Option<PathBuf>,

    /// A path to the script to run in Roblox Studio.
    ///
    /// The script will be run at plugin-level security.
    #[arg(long = "script")]
    script_path: PathBuf,

    /// One or more regular expressions. If an error message matches any of
    /// these regexes, the error will be logged but the program will exit with
    /// code 0.
    #[arg(long = "ignore-pattern")]
    ignore_pattern: Vec<String>,
}
fn run(options: &Options) -> Result<i32, anyhow::Error> {
    // Create a temp directory to house our place, even if a path is given from
    // the command line. This helps ensure Studio won't hang trying to tell the
    // user that the place is read-only because of a .lock file.
    let temp_place_folder = tempdir()?;
    let temp_place_path;

    match &options.place_path {
        Some(place_path) => {
            let extension = place_path
                .extension()
                .ok_or_else(|| anyhow!("Place file did not have a file extension"))?
                .to_str()
                .ok_or_else(|| anyhow!("Place file extension had invalid Unicode"))?;

            temp_place_path = temp_place_folder
                .path()
                .join(format!("run-in-roblox-place.{}", extension));

            fs::copy(place_path, &temp_place_path)?;
        }
        None => {
            unimplemented!("run-in-roblox with no place argument");
        }
    }

    let script_contents = fs::read_to_string(&options.script_path)?;

    // Compile ignore patterns into regexes early so we can use them when
    // emitting errors. Invalid regexes will cause an immediate error.
    let ignore_regexes: Vec<regex::Regex> = options
        .ignore_pattern
        .iter()
        .map(|s| regex::Regex::new(s))
        .collect::<Result<_, _>>()?;

    // Generate a random, unique ID for this session. The plugin we inject will
    // compare this value with the one reported by the server and abort if they
    // don't match.
    let server_id = format!("run-in-roblox-{:x}", rand::random::<u128>());

    let place_runner = PlaceRunner {
        port: 50312,
        place_path: temp_place_path.clone(),
        server_id: server_id.clone(),
        lua_script: script_contents.clone(),
    };

    let (sender, receiver) = mpsc::channel();

    thread::spawn(move || {
        place_runner.run(sender).unwrap();
    });

    let mut exit_code = 0;

    while let Some(message) = receiver.recv()? {
        match message {
            RobloxMessage::Output { level, body } => {
                let colored_body = match level {
                    OutputLevel::Print => body.normal(),
                    OutputLevel::Info => body.cyan(),
                    OutputLevel::Warning => body.yellow(),
                    OutputLevel::Error => body.red(),
                };

                println!("{}", colored_body);

                if level == OutputLevel::Error {
                    // If any ignore regex matches the error body, treat as success.
                    if ignore_regexes.iter().any(|re| re.is_match(&body)) {
                        log::warn!("Ignored error by pattern: {}", body);
                    } else {
                        exit_code = 1;
                    }
                }
            }
        }
    }

    Ok(exit_code)
}

fn main() {
    let options = Options::parse();

    {
        let log_env = env_logger::Env::default().default_filter_or("warn");

        env_logger::Builder::from_env(log_env)
            .format_timestamp(None)
            .init();
    }

    match run(&options) {
        Ok(exit_code) => process::exit(exit_code),
        Err(err) => {
            // If any ignore pattern matches the error, log and exit 0.
            let msg = format!("{:?}", err);
            if options
                .ignore_pattern
                .iter()
                .filter_map(|p| regex::Regex::new(p).ok())
                .any(|re| re.is_match(&msg))
            {
                log::warn!("Ignored error by pattern: {}", msg);
                process::exit(0);
            }

            log::error!("{}", msg);
            process::exit(2);
        }
    }
}
