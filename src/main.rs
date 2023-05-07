use std::{collections::HashMap, error::Error, fs::File};

use serde::{Deserialize, Serialize};
use swayipc::{Connection, Event, EventType, Fallible, Node, NodeType, WindowChange};

#[derive(Deserialize, Serialize, Default, Debug)]
struct SwayNameManagerConfig {
    app_symbols: HashMap<String, String>,
}

struct SwayNameManager {
    config: SwayNameManagerConfig,
}

trait Autorename {
    fn contains(&self, node: &Node) -> bool;
    fn get_workspace<'a>(&'a self, node: &'a Node) -> Result<&'a Node, Box<dyn Error>>;
    fn get_workspace_nodes(&self) -> Vec<&Node>;
    fn get_window_names(&self) -> Vec<String>;
    fn update_workspace_names(&self, name_config: &SwayNameManagerConfig);
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
            .map(|node| *node)
            .collect();

        if nodes.len() == 1 {
            Ok(nodes.get(0).unwrap())
        } else {
            return Err("Window is on multiple workspaces!".into());
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

    fn update_workspace_names(&self, name_config: &SwayNameManagerConfig) {
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
                        let new_name;
                        let mapped_name = name_config.app_symbols.get(name);
                        match mapped_name {
                            Some(symbol) => {
                                new_name = symbol.clone();
                            }
                            None => new_name = name.clone(),
                        }
                        new_name
                    })
                    .rev()
                    .collect();
                let new_name;
                // Special case if the list is empty
                if window_names.len() == 0 {
                    new_name = format!("{}", workspace_num);
                } else {
                    new_name = format!("{}: {}", workspace_num, window_names.join("|"));
                }
                let old_name = node.name.clone().unwrap_or("".to_string());
                // Only send the command if the new name differs
                if new_name != old_name {
                    let mut sway_connection = Connection::new().unwrap();
                    let rename_commands =
                        format!("rename workspace \"{}\" to \"{}\"", old_name, new_name);
                    sway_connection.run_command(rename_commands).unwrap();
                }
            }
        }
    }
}

impl SwayNameManager {
    fn run(&mut self) -> Fallible<()> {
        let root_node = Connection::new()?.get_tree()?;
        root_node.update_workspace_names(&self.config);
        let subs = [EventType::Window];
        let sway_connection = Connection::new()?;
        for e in sway_connection.subscribe(subs)? {
            match e {
                Ok(event) => match event {
                    Event::Window(windowevent) => match windowevent.change {
                        // TODO: On New we don't need to update all of them
                        WindowChange::New | WindowChange::Close | WindowChange::Move => {
                            //let _ = Self::handle_event(&windowevent.container);
                            let root_node = Connection::new()?.get_tree()?;
                            root_node.update_workspace_names(&self.config);
                        }
                        _ => {}
                    },

                    _ => {}
                },
                Err(err) => {
                    println!("Error in event: {}", err);
                }
            }
        }
        Ok(())
    }

    fn new(config_path: String) -> Self {
        let mut config = SwayNameManagerConfig {
            ..Default::default()
        };
        let file_result = File::open(config_path);
        match file_result {
            Ok(config_file) => {
                let serde_result = serde_json::from_reader(config_file);
                match serde_result {
                    Ok(result) => {
                        config = result;
                    }
                    Err(e) => println!("Error while reading config: {}. Using default config", e),
                }
            }
            Err(e) => {
                println!("Failed to open config file: {}. Using default config", e);
            }
        }
        Self { config }
    }
}

fn main() -> Fallible<()> {
    let mut manager = SwayNameManager::new("config.json".to_string());
    manager.run().unwrap();
    Ok(())
}
