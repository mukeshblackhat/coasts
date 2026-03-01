/// Polls Docker connectivity and emits `DockerStatusChanged` events
/// when the daemon comes up or goes down. Polls every 5 seconds.
use std::sync::Arc;

use tracing::debug;

use coast_core::protocol::CoastEvent;

use crate::server::AppState;

pub fn spawn_docker_watcher(state: Arc<AppState>) {
    tokio::spawn(async move {
        let mut interval = tokio::time::interval(std::time::Duration::from_secs(5));
        interval.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);

        let mut was_connected: Option<bool> = None;

        loop {
            interval.tick().await;

            let connected = match state.docker.as_ref() {
                Some(docker) => docker.ping().await.is_ok(),
                None => false,
            };

            if was_connected != Some(connected) {
                debug!(connected, "docker status changed");
                state.emit_event(CoastEvent::DockerStatusChanged { connected });
                was_connected = Some(connected);
            }
        }
    });
}
