use std::{
    error::Error,
    sync::{Arc, RwLock},
};

use anyhow::anyhow;
use futures_util::StreamExt;
use log::error;
use swayipc_async::{Connection, Event, EventType, Fallible, Node, NodeType, WindowChange};

use crate::{SwayNameManager, WindowManager, config::SwayNameManagerConfig};

trait Autorename {
    #[allow(dead_code)]
    fn contains(&self, node: &Node) -> bool;
    #[allow(dead_code)]
    fn get_workspace<'a>(&'a self, node: &'a Node) -> Result<&'a Node, Box<dyn Error>>;
    #[allow(dead_code)]
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
            Ok(nodes.first().unwrap())
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
                    .map(|name| name_config.get_symbol(name))
                    .rev()
                    .collect();
                // Special case if the list is empty

                let new_name = if window_names.is_empty() {
                    format!("{workspace_num}")
                } else {
                    format!("{}: {}", workspace_num, window_names.join("|"))
                };
                let old_name = node.name.clone().unwrap_or_default();
                // Only send the command if the new name differs
                if new_name != old_name {
                    let mut sway_connection = Connection::new().await.unwrap();
                    let rename_commands =
                        format!("rename workspace \"{old_name}\" to \"{new_name}\"",);
                    sway_connection.run_command(rename_commands).await.unwrap();
                }
            }
        }
    }
}

impl WindowManager for SwayNameManager {
    fn update_workspace(&self, id: i32, name: &str) -> anyhow::Result<()> {
        // TODO: make everything async
        futures::executor::block_on(async {
            let mut connection = Connection::new().await.unwrap();
            let workspaces = connection.get_workspaces().await.unwrap();

            let workspace = workspaces
                .iter()
                .find(|w| w.num == id)
                .ok_or(anyhow!("not found"))
                .unwrap();
            let old_name = workspace.name.clone();
            let rename_commands = format!("rename workspace \"{old_name}\" to \"{name}\"",);
            connection.run_command(rename_commands).await.unwrap();
        });

        Ok(())
    }

    fn get_workspaces(&self) -> anyhow::Result<Vec<i32>> {
        let result = futures::executor::block_on(async {
            let mut connection = Connection::new().await.unwrap();
            let workspaces = connection.get_workspaces().await.unwrap();
            workspaces.iter().map(|w| w.num).collect()
        });
        Ok(result)
    }

    fn get_workspace_name(&self, id: i32) -> anyhow::Result<String> {
        let result = futures::executor::block_on(async {
            let root_node = Connection::new().await.unwrap().get_tree().await.unwrap();
            let mut nodes_to_search: Vec<&Node> = vec![&root_node];
            // Iterate over self including all children
            while let Some(node) = nodes_to_search.pop() {
                node.nodes.iter().for_each(|child_node| {
                    nodes_to_search.push(child_node);
                });
                // Build new name if we have a workspace. Scratchpad is ignored since it doesn' have a
                // number
                if node.node_type == NodeType::Workspace
                    && let Some(workspace_node) = node.num
                    && workspace_node == id
                {
                    // Get the window names and map them according to the config. If no match
                    // exists we use the id of the window
                    let window_names: Vec<String> = node
                        .get_window_names()
                        .iter()
                        .map(|name| self.config.read().unwrap().get_symbol(name))
                        .rev()
                        .collect();
                    return window_names.join("|");
                }
            }
            String::new()
        });
        Ok(result)
    }
}

impl SwayNameManager {
    pub async fn run(&mut self) -> Fallible<()> {
        let config = self.config.read().unwrap().clone();
        let root_node = Connection::new().await?.get_tree().await?;
        root_node.update_workspace_names(&config).await;
        let subs = [EventType::Window];
        let sway_connection = Connection::new().await?;
        let mut events = sway_connection.subscribe(subs).await?;
        while let Some(event) = events.next().await {
            match event {
                Ok(event) => {
                    if let Event::Window(windowevent) = event {
                        match windowevent.change {
                            // TODO: On New we don't need to update all of them
                            WindowChange::New | WindowChange::Close | WindowChange::Move => {
                                let _ = self.update_all();
                            }
                            _ => {}
                        }
                    }
                }
                Err(err) => {
                    error!("Error in event: {err}");
                }
            }
        }
        Ok(())
    }

    pub fn new(config: Arc<RwLock<SwayNameManagerConfig>>) -> Self {
        Self { config }
    }
}
