use std::{
    path::PathBuf,
    sync::{Arc, RwLock},
};

use anyhow::Result;
use futures_util::stream::StreamExt;
use inotify::{EventMask, Inotify, WatchMask};
use log::*;
use simplelog::{ColorChoice, Config, LevelFilter, TermLogger, TerminalMode};

use clap::Parser;

use crate::config::SwayNameManagerConfig;

mod config;
mod wm;

struct SwayNameManager {
    config: Arc<RwLock<SwayNameManagerConfig>>,
}

trait WindowManager {
    fn get_workspaces(&self) -> Result<Vec<i32>>;
    fn get_workspace_name(&self, id: i32) -> Result<String>;
    fn update_workspace(&self, id: i32, name: &str) -> Result<()>;

    fn update_all(&self) -> Result<()> {
        let workspaces = self.get_workspaces()?;
        for i in workspaces {
            let name = self.get_workspace_name(i)?;
            self.update_workspace(i, &format!("{i}:{name}"))?;
        }
        Ok(())
    }
}

#[derive(clap::ValueEnum, Debug, Clone, PartialEq, Eq)]
pub enum WindowManagerType {
    #[cfg(feature = "hyprland")]
    Hyprland,
    #[cfg(feature = "sway")]
    Sway,
    All,
}

#[derive(Parser, Debug)]
#[command(author, version, about, long_about = None)]
struct Args {
    /// Config to load
    #[arg(short, long)]
    config: Option<PathBuf>,

    #[arg(short, long)]
    window_manager: WindowManagerType,
}

fn get_config_paths(aditional_paths: &Option<PathBuf>) -> Vec<PathBuf> {
    let config_dir = dirs::config_dir().unwrap_or_default();
    let home_config = config_dir.join("swayautonames/config.json");
    let mut config_search_paths = vec![];

    if let Some(p) = aditional_paths {
        config_search_paths.push(p.clone());
    }

    config_search_paths.push(PathBuf::from("./config.json"));
    config_search_paths.push(home_config);
    config_search_paths.push(PathBuf::from("/etc/swayautonames/config.json"));

    config_search_paths
}

fn get_config_path(aditional_paths: Option<PathBuf>) -> Option<PathBuf> {
    let config_search_paths = get_config_paths(&aditional_paths);
    let selected_config;
    if let Some(config_path) = aditional_paths {
        selected_config = Some(PathBuf::from(&config_path));
    } else {
        let existing_config = config_search_paths
            .iter()
            .find(|config_path| {
                info!("Testing {config_path:?}");
                config_path.exists()
            })
            .cloned();
        selected_config = existing_config;
    }
    selected_config
}

#[tokio::main]
async fn main() -> Result<()> {
    TermLogger::init(
        LevelFilter::Trace,
        Config::default(),
        TerminalMode::Stdout,
        ColorChoice::Auto,
    )
    .unwrap();
    let args = Args::parse();
    let selected_config_path = get_config_path(args.config);
    info!("Starting swayautonames with config: {selected_config_path:?}");
    let config = Arc::new(RwLock::new(SwayNameManagerConfig::from_file(
        &selected_config_path.clone().unwrap_or_default(),
    )));
    #[cfg(feature = "sway")]
    if args.window_manager == WindowManagerType::Sway
        || args.window_manager == WindowManagerType::All
    {
        let mut manager = SwayNameManager::new(config.clone());
        tokio::spawn(async move {
            manager.run().await.unwrap();
        });
    }
    #[cfg(feature = "hyprland")]
    if args.window_manager == WindowManagerType::Hyprland
        || args.window_manager == WindowManagerType::All
    {
        let hyprland_config = config.clone();
        tokio::spawn(async move {
            loop {
                let res = wm::hyprland::HyprlandManager {
                    config: hyprland_config.clone(),
                }
                .run()
                .await;
                match res {
                    Ok(_) => break,
                    Err(err) => {
                        error!("Recreating because HyprlandManager returned with error: {err}");
                    }
                }
            }
        });
    }
    if let Some(config_path) = &selected_config_path {
        let inotify = Inotify::init()?;
        let mask = WatchMask::MODIFY | WatchMask::CREATE | WatchMask::DELETE_SELF;
        inotify.watches().add(config_path, mask)?;

        let mut buffer = [0; 1024];
        let mut stream = inotify.into_event_stream(&mut buffer)?;

        while let Some(event_or_error) = stream.next().await {
            if let Ok(event) = event_or_error
                && event.mask.contains(EventMask::DELETE_SELF)
            {
                // Recreate inotify. Some editors delete the file and recreate it (e.g. neovim)
                stream.watches().add(config_path, mask)?;
            }
            let new_config = SwayNameManagerConfig::from_file(config_path);
            *config.write().unwrap() = new_config.clone();
        }
    }
    Ok(())
}
