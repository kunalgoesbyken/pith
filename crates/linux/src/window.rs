use atspi::proxy::accessible::ObjectRefExt;
use atspi::{AtspiError, ObjectRefOwned, Role};
use zbus::Connection;

const MAX_ANCESTORS: usize = 64;

pub(crate) async fn title(
    connection: &Connection,
    field: &ObjectRefOwned,
) -> Result<String, AtspiError> {
    let mut current = field.clone();
    for _ in 0..MAX_ANCESTORS {
        let node = current.as_accessible_proxy(connection).await?;
        match node.get_role().await? {
            Role::Frame | Role::Window | Role::Dialog => return Ok(node.name().await?),
            Role::Application => break,
            _ => {}
        }
        let parent = node.parent().await?;
        if parent.is_null() {
            break;
        }
        current = parent;
    }
    Ok(String::new())
}
