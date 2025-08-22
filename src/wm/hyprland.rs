use std::error::Error;
use std::sync::{Arc, RwLock};

use anyhow::{anyhow, Result};
use hyprland::dispatch::{Dispatch, DispatchType};
use hyprland::prelude::*;
use hyprland::{data::*, event_listener::EventListener};

use crate::config::SwayNameManagerConfig;
use crate::WindowManager;

pub struct HyprlandManager {
    pub config: Arc<RwLock<SwayNameManagerConfig>>,
}

impl WindowManager for HyprlandManager {
    fn get_workspaces(&self) -> Result<Vec<i32>> {
        Ok(Workspaces::get()?.iter().map(|w| w.id).collect())
    }
    fn get_workspace_name(&self, id: i32) -> Result<String> {
        let workspaces = Workspaces::get()?.to_vec();
        let clients = Clients::get()?.to_vec();
        let workspace = workspaces
            .iter()
            .find(|w| w.id == id)
            .ok_or(anyhow!("not found"))?;
        let workspace_clients = clients.iter().filter(|c| c.workspace.id == workspace.id);
        let names: Vec<String> = workspace_clients
            .map(|client| {
                self.config
                    .read()
                    .unwrap()
                    .app_symbols
                    .get(&client.class.clone())
                    .unwrap_or(&client.class.clone())
                    .clone()
            })
            .collect();
        let new_name = names.join("|");
        Ok(new_name)
    }

    fn update_workspace(&self, id: i32, name: &str) -> Result<()> {
        Dispatch::call(DispatchType::RenameWorkspace(id, Some(name)))?;
        Ok(())
    }
}

impl HyprlandManager {
    fn update(config: Arc<RwLock<SwayNameManagerConfig>>) -> Result<(), Box<dyn Error>> {
        HyprlandManager { config }.update_all()?;

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
