use crate::tui::{TuiActor, TuiMsg};
use nowhere_actors::actor::Addr;
use nowhere_actors::system::ShutdownHandle;
use std::time::Duration;
use tokio::{self, time};

pub fn spawn_tui_feeders(tui: Addr<TuiActor>, shutdown: ShutdownHandle) {
    let tui_in = tui.clone();
    let mut shutdown_input = shutdown.subscribe();
    tokio::spawn(async move {
        loop {
            tokio::select! {
                res = shutdown_input.recv() => {
                    if res.is_err() {
                        break;
                    }
                    break;
                }
                // FIXME: reuse a dedicated blocking thread instead of spawning a task per keypress to reduce allocator pressure.
                ev = tokio::task::spawn_blocking(|| crossterm::event::read()) => {
                    match ev {
                        Ok(Ok(e)) => {
                            let _ = tui_in.send(TuiMsg::InputEvent(e)).await;
                        }
                        Ok(Err(e)) => {
                            let _ = tui_in.send(TuiMsg::OpError(format!("input: {e}"))).await;
                        }
                        Err(_) => break,
                    }
                }
            }
        }
    });

    let tui_tick = tui.clone();
    let mut shutdown_tick = shutdown.subscribe();
    tokio::spawn(async move {
        let mut interval = time::interval(Duration::from_millis(80));
        loop {
            tokio::select! {
                res = shutdown_tick.recv() => {
                    if res.is_err() {
                        break;
                    }
                    break;
                }
                _ = interval.tick() => {
                    let _ = tui_tick.try_send(TuiMsg::Tick);
                }
            }
        }
    });
}
