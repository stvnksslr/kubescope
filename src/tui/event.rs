use std::time::Duration;

use crossterm::event::{self, Event as CrosstermEvent, KeyEvent, KeyEventKind};
use futures::{FutureExt, StreamExt};
use tokio::sync::mpsc;
use tokio_util::sync::CancellationToken;

/// Terminal events
#[derive(Clone, Debug)]
#[allow(dead_code)]
pub enum Event {
    /// Terminal tick (for periodic updates)
    Tick,
    /// Key press event
    Key(KeyEvent),
    /// Terminal resize
    Resize(u16, u16),
    /// Error occurred
    Error(String),
}

/// Event handler managing terminal input
pub struct EventHandler {
    /// Event receiver
    receiver: mpsc::UnboundedReceiver<Event>,
    /// Cancellation token for graceful shutdown
    cancel: CancellationToken,
    /// Task handle
    #[allow(dead_code)]
    task: tokio::task::JoinHandle<()>,
}

impl EventHandler {
    /// Create a new event handler with the given tick rate
    pub fn new(tick_rate: Duration) -> Self {
        let (sender, receiver) = mpsc::unbounded_channel();
        let cancel = CancellationToken::new();

        let task = {
            let sender = sender.clone();
            let cancel = cancel.clone();

            tokio::spawn(async move {
                let mut reader = event::EventStream::new();
                let mut tick_interval = tokio::time::interval(tick_rate);

                loop {
                    let tick = tick_interval.tick();
                    let crossterm_event = reader.next().fuse();

                    tokio::select! {
                        _ = cancel.cancelled() => break,

                        _ = tick => {
                            let _ = sender.send(Event::Tick);
                        }

                        maybe_event = crossterm_event => {
                            match maybe_event {
                                Some(Ok(evt)) => {
                                    match evt {
                                        CrosstermEvent::Key(key) => {
                                            // Filter out release events (important for Windows)
                                            if key.kind == KeyEventKind::Press {
                                                let _ = sender.send(Event::Key(key));
                                            }
                                        }
                                        CrosstermEvent::Resize(w, h) => {
                                            let _ = sender.send(Event::Resize(w, h));
                                        }
                                        _ => {}
                                    }
                                }
                                Some(Err(e)) => {
                                    let _ = sender.send(Event::Error(e.to_string()));
                                }
                                None => break,
                            }
                        }
                    }
                }
            })
        };

        Self {
            receiver,
            cancel,
            task,
        }
    }

    /// Receive the next event
    pub async fn next(&mut self) -> Option<Event> {
        self.receiver.recv().await
    }

    /// Shutdown the event handler
    pub fn shutdown(&self) {
        self.cancel.cancel();
    }
}
