use std::{
    collections::HashMap,
    error::Error,
    fs::File,
    path::PathBuf,
    sync::{Arc, RwLock},
};

use futures_util::stream::StreamExt;
use inotify::{EventMask, Inotify, WatchMask};
use log::*;
use serde::{Deserialize, Serialize};
use simplelog::{ColorChoice, Config, LevelFilter, TermLogger, TerminalMode};
use swayipc_async::{Connection, Event, EventType, Fallible, Node, NodeType, WindowChange};

use clap::Parser;

#[derive(Deserialize, Serialize, Default, Debug, Clone)]
struct SwayNameManagerConfig {
    app_symbols: HashMap<String, String>,
}

impl SwayNameManagerConfig {
    fn from_file(config_path: &PathBuf) -> Self {
        let file_result = File::open(config_path);
        match file_result {
            Ok(config_file) => {
                let serde_result = serde_yaml::from_reader(config_file);
                match serde_result {
                    Ok(result) => {
                        return result;
                    }
                    Err(e) => {
                        error!("Error while reading config: {}. Using default config", e)
                    }
                }
            }
            Err(e) => {
                error!("Failed to open config file: {}. Using default config", e);
            }
        }
        Self {
            ..Default::default()
        }
    }
}

struct SwayNameManager {
    config: Arc<RwLock<SwayNameManagerConfig>>,
}

trait Autorename {
    fn contains(&self, node: &Node) -> bool;
    fn get_workspace<'a>(&'a self, node: &'a Node) -> Result<&'a Node, Box<dyn Error>>;
    fn get_workspace_nodes(&self) -> Vec<&Node>;
    fn get_window_names(&self) -> Vec<String>;
    async fn update_workspace_names(&self, name_config: &SwayNameManagerConfig);
}

impl Autorename for Node {
    fn contains(&self, node: &Node) -> bool {
        self.id == node.id || self.nodes.iter().any(|child| child.contains(node))
    }

    fn get_workspace<'a>(&'a self, node: &'a Node) -> Result<&'a Node, Box<dyn Error>> {
        let workspaces = self.get_workspace_nodes();

        let nodes: Vec<&Node> = workspaces
            .iter()
            .filter(|workspace| workspace.contains(node))
            .copied()
            .collect();

        if nodes.len() == 1 {
            Ok(nodes.get(0).unwrap())
        } else {
            Err("Window is on multiple workspaces!".into())
        }
    }

    fn get_workspace_nodes(&self) -> Vec<&Node> {
        let mut nodes_to_search: Vec<&Node> = vec![self];
        let mut workspace_nodes = vec![];

        while let Some(node) = nodes_to_search.pop() {
            match node.node_type {
                NodeType::Workspace => {
                    workspace_nodes.push(node);
                }
                _ => {
                    node.nodes
                        .iter()
                        .for_each(|child_node| nodes_to_search.push(child_node));
                }
            }
        }
        workspace_nodes
    }
    fn get_window_names(&self) -> Vec<String> {
        let mut nodes_to_search: Vec<&Node> = vec![self];
        let mut names = vec![];
        while let Some(node) = nodes_to_search.pop() {
            if node.node_type == NodeType::Con {
                // App_id on wayland
                if let Some(name) = &node.app_id {
                    names.push(name.clone());
                } else {
                    // Use the instance for xwayland applications
                    let instance = node.window_properties.clone().and_then(|o| o.instance);
                    if let Some(name) = instance {
                        names.push(name);
                    }
                }
            }
            node.nodes
                .iter()
                .for_each(|child_node| nodes_to_search.push(child_node));
        }
        names
    }

    async fn update_workspace_names(&self, name_config: &SwayNameManagerConfig) {
        let mut nodes_to_search: Vec<&Node> = vec![self];
        // Iterate over self including all children
        while let Some(node) = nodes_to_search.pop() {
            node.nodes.iter().for_each(|child_node| {
                nodes_to_search.push(child_node);
            });
            // Build new name if we have a workspace. Scratchpad is ignored since it doesn' have a
            // number
            if node.node_type == NodeType::Workspace && node.num.is_some() {
                let workspace_num = node.num.unwrap();
                // Get the window names and map them according to the config. If no match
                // exists we use the id of the window
                let window_names: Vec<String> = node
                    .get_window_names()
                    .iter()
                    .map(|name| {
                        let mapped_name = name_config.app_symbols.get(name);
                        match mapped_name {
                            Some(symbol) => symbol.clone(),
                            None => name.clone(),
                        }
                    })
                    .rev()
                    .collect();
                // Special case if the list is empty

                let new_name = if window_names.is_empty() {
                    format!("{}", workspace_num)
                } else {
                    format!("{}: {}", workspace_num, window_names.join("|"))
                };
                let old_name = node.name.clone().unwrap_or_default();
                // Only send the command if the new name differs
                if new_name != old_name {
                    let mut sway_connection = Connection::new().await.unwrap();
                    let rename_commands =
                        format!("rename workspace \"{}\" to \"{}\"", old_name, new_name);
                    sway_connection.run_command(rename_commands).await.unwrap();
                }
            }
        }
    }
}

impl SwayNameManager {
    async fn run(&mut self) -> Fallible<()> {
        let config = self.config.read().unwrap().clone();
        let root_node = Connection::new().await?.get_tree().await?;
        root_node.update_workspace_names(&config).await;
        let subs = [EventType::Window];
        let sway_connection = Connection::new().await?;
        let mut events = sway_connection.subscribe(subs).await?;
        while let Some(event) = events.next().await {
            match event {
                Ok(event) => match event {
                    Event::Window(windowevent) => match windowevent.change {
                        // TODO: On New we don't need to update all of them
                        WindowChange::New | WindowChange::Close | WindowChange::Move => {
                            //let _ = Self::handle_event(&windowevent.container);
                            let root_node = Connection::new().await?.get_tree().await?;
                            let config = self.config.read().unwrap().clone();
                            root_node.update_workspace_names(&config).await;
                        }
                        _ => {}
                    },

                    _ => {}
                },
                Err(err) => {
                    error!("Error in event: {}", err);
                }
            }
        }
        Ok(())
    }

    fn new(config_path: Option<PathBuf>) -> Self {
        let mut config = SwayNameManagerConfig {
            ..Default::default()
        };

        if let Some(config_path) = config_path {
            config = SwayNameManagerConfig::from_file(&config_path);
        }

        Self {
            config: Arc::new(RwLock::new(config)),
        }
    }
}

#[derive(Parser, Debug)]
#[command(author, version, about, long_about = None)]
struct Args {
    /// Config to load
    #[arg(short, long)]
    config: Option<PathBuf>,
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

fn get_config(aditional_paths: Option<PathBuf>) -> Option<PathBuf> {
    let config_search_paths = get_config_paths(&aditional_paths);
    let selected_config;
    if let Some(config_path) = aditional_paths {
        selected_config = Some(PathBuf::from(&config_path));
    } else {
        let existing_config = config_search_paths
            .iter()
            .find(|config_path| {
                info!("Testing {:?}", config_path);
                config_path.exists()
            })
            .cloned();
        selected_config = existing_config;
    }
    selected_config
}

#[tokio::main]
async fn main() -> Fallible<()> {
    TermLogger::init(
        LevelFilter::Trace,
        Config::default(),
        TerminalMode::Stdout,
        ColorChoice::Auto,
    )
    .unwrap();
    let args = Args::parse();
    let selected_config = get_config(args.config);
    info!("Starting swayautonames with config: {:?}", selected_config);
    let mut manager = SwayNameManager::new(selected_config.clone());
    let manager_config = manager.config.clone();
    tokio::spawn(async move {
        manager.run().await.unwrap();
    });
    if let Some(config) = &selected_config {
        let inotify = Inotify::init()?;
        let mask = WatchMask::MODIFY | WatchMask::CREATE | WatchMask::DELETE_SELF;
        inotify.watches().add(config, mask)?;

        let mut buffer = [0; 1024];
        let mut stream = inotify.into_event_stream(&mut buffer)?;

        while let Some(event_or_error) = stream.next().await {
            if let Ok(event) = event_or_error {
                if event.mask.contains(EventMask::DELETE_SELF) {
                    // Recreate inotify. Some editors delete the file and recreate it (e.g. neovim)
                    stream.watches().add(config, mask)?;
                }
            }
            let new_config = SwayNameManagerConfig::from_file(&config);
            *manager_config.write().unwrap() = new_config.clone();
            let root_node = Connection::new().await?.get_tree().await?;
            root_node.update_workspace_names(&new_config).await;
        }
    }
    Ok(())
}
