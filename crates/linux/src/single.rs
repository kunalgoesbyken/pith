use std::time::Duration;

use async_io::Timer;
use futures_lite::future::block_on;
use pith_core::{TrayCommand, log};
use zbus::Connection;
use zbus::connection::Builder;

use crate::bounded::{DEADLINE, within};
use crate::run::{Inbound, Inbox};

const NAME: &str = "org.pith.Pith";
const PATH: &str = "/org/pith/Pith";
const OPEN: &str = "Open";
const ATTEMPTS: usize = 3;
const RETRY_DELAY: Duration = Duration::from_millis(200);

struct Remote {
    inbox: Inbox,
}

#[zbus::interface(name = "org.pith.Pith")]
impl Remote {
    fn open(&self) {
        self.inbox.post(Inbound::Command(TrayCommand::Open));
    }
}

pub struct Instance {
    _connection: Option<Connection>,
}

async fn reveal() -> zbus::Result<()> {
    let connection = Connection::session().await?;
    connection
        .call_method(Some(NAME), PATH, Some(NAME), OPEN, &())
        .await?;
    Ok(())
}

async fn serve(inbox: Inbox) -> zbus::Result<Connection> {
    Builder::session()?
        .name(NAME)?
        .allow_name_replacements(false)
        .replace_existing_names(false)
        .serve_at(PATH, Remote { inbox })?
        .build()
        .await
}

async fn claim(inbox: Inbox) -> zbus::Result<Option<Instance>> {
    for _ in 0..ATTEMPTS {
        let connection = match serve(inbox.clone()).await {
            Ok(connection) => Some(connection),
            Err(zbus::Error::NameTaken) => {
                if let Some(Ok(())) = within(DEADLINE, reveal()).await {
                    return Ok(None);
                }
                Timer::after(RETRY_DELAY).await;
                continue;
            }
            Err(error) => {
                log(&format!(
                    "single instance not checked, running alone: {error}"
                ));
                None
            }
        };
        return Ok(Some(Instance {
            _connection: connection,
        }));
    }
    Err(zbus::Error::NameTaken)
}

pub fn single_instance(inbox: Inbox) -> zbus::Result<Option<Instance>> {
    block_on(claim(inbox))
}
