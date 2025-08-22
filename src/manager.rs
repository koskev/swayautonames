use std::collections::HashMap;

use serde::{Deserialize, Serialize};
use swayipc_async::Fallible;

#[derive(Deserialize, Serialize, Default, Debug, Clone)]
pub struct SwayNameManagerConfig {
    app_symbols: HashMap<String, String>,
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
                Ok(event) => {
                    if let Event::Window(windowevent) = event {
                        match windowevent.change {
                            // TODO: On New we don't need to update all of them
                            WindowChange::New | WindowChange::Close | WindowChange::Move => {
                                //let _ = Self::handle_event(&windowevent.container);
                                let root_node = Connection::new().await?.get_tree().await?;
                                let config = self.config.read().unwrap().clone();
                                root_node.update_workspace_names(&config).await;
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
