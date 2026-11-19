use std::time::Duration;

use async_io::Timer;
use futures_lite::future::block_on;
use pith_core::TrayCommand;
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
    _connection: Connection,
}

async fn reveal() -> zbus::Result<()> {
    let connection = Connection::session().await?;
    connection
        .call_method(Some(NAME), PATH, Some(NAME), OPEN, &())
        .await?;
    Ok(())
}

async fn serve(inbox: Inbox) -> zbus::Result<Instance> {
    let connection = Builder::session()?
        .name(NAME)?
        .serve_at(PATH, Remote { inbox })?
        .build()
        .await?;
    Ok(Instance {
        _connection: connection,
    })
}

async fn claim(inbox: Inbox) -> zbus::Result<Option<Instance>> {
    for _ in 0..ATTEMPTS {
        match serve(inbox.clone()).await {
            Ok(instance) => return Ok(Some(instance)),
            Err(zbus::Error::NameTaken) => {}
            Err(error) => return Err(error),
        }
        if let Some(Ok(())) = within(DEADLINE, reveal()).await {
            return Ok(None);
        }
        Timer::after(RETRY_DELAY).await;
    }
    Err(zbus::Error::NameTaken)
}

pub fn single_instance(inbox: Inbox) -> zbus::Result<Option<Instance>> {
    block_on(claim(inbox))
}
