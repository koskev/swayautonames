use std::error::Error;
use std::sync::{Arc, RwLock};

use anyhow::Result;
use hyprland::dispatch::{Dispatch, DispatchType};
use hyprland::prelude::*;
use hyprland::{data::*, event_listener::EventListener};

use crate::config::SwayNameManagerConfig;

pub struct HyprlandManager {
    pub config: Arc<RwLock<SwayNameManagerConfig>>,
}

impl HyprlandManager {
    fn update(config: Arc<RwLock<SwayNameManagerConfig>>) -> Result<(), Box<dyn Error>> {
        let workspaces = Workspaces::get()?.to_vec();
        let config = config.read().unwrap();

        let clients = Clients::get()?.to_vec();
        for workspace in workspaces {
            let workspace_clients = clients.iter().filter(|c| c.workspace.id == workspace.id);
            let names: Vec<String> = workspace_clients
                .map(|client| {
                    config
                        .app_symbols
                        .get(&client.class.clone())
                        .unwrap_or(&client.class.clone())
                        .clone()
                })
                .collect();
            let new_name = names.join("|");

            Dispatch::call(DispatchType::RenameWorkspace(workspace.id, Some(&new_name)))?
        }

        Ok(())
    }
    pub async fn run(&self) -> Result<()> {
        // Create a event listener
        let mut event_listener = EventListener::new();
        let config = self.config.clone();

        event_listener.add_window_opened_handler(move |_| {
            Self::update(config.clone()).unwrap();
        });
        let config = self.config.clone();
        event_listener.add_window_moved_handler(move |_| {
            Self::update(config.clone()).unwrap();
        });
        let config = self.config.clone();
        event_listener.add_window_closed_handler(move |_| {
            Self::update(config.clone()).unwrap();
        });
        event_listener.start_listener()?;

        Ok(())
    }
}
