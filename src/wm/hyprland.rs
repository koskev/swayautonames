use std::error::Error;
use std::sync::{Arc, RwLock};

use anyhow::{Result, anyhow};
use hyprland::dispatch::{Dispatch, DispatchType};
use hyprland::prelude::*;
use hyprland::{data::*, event_listener::EventListener};

use crate::WindowManager;
use crate::config::SwayNameManagerConfig;

pub struct HyprlandManager {
    pub config: Arc<RwLock<SwayNameManagerConfig>>,
}

impl WindowManager for HyprlandManager {
    fn get_workspaces(&self) -> Result<Vec<i32>> {
        Ok(Workspaces::get()?.iter().map(|w| w.id).collect())
    }
    fn get_workspace_name(&self, id: i32) -> Result<String> {
        let config = self.config.read().unwrap();
        let workspaces = Workspaces::get()?.to_vec();
        let clients = Clients::get()?.to_vec();
        let workspace = workspaces
            .iter()
            .find(|w| w.id == id)
            .ok_or(anyhow!("not found"))?;
        let workspace_clients = clients.iter().filter(|c| c.workspace.id == workspace.id);
        let names: Vec<String> = workspace_clients
            .map(|client| {
                let name = config.get_symbol(&client.class);
                if let Some(color) = &config.fullscreen_color
                    && client.fullscreen != FullscreenMode::None
                {
                    // XXX: Waybar does not support selecting the text with css
                    format!(r#"<span foreground="{color}">{name}</span>"#)
                } else {
                    name
                }
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
        let config = self.config.clone();
        event_listener.add_fullscreen_state_changed_handler(move |_| {
            Self::update(config.clone()).unwrap();
        });
        event_listener.start_listener()?;

        Ok(())
    }
}
