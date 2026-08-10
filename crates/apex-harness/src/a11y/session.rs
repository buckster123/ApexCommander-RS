//! AT-SPI connection lifecycle and readiness probe.

use atspi::connection::AccessibilityConnection;
use atspi::proxy::accessible::{AccessibleProxy, ObjectRefExt};
use atspi::ObjectRef;
use tracing::{debug, warn};
use zbus::names::UniqueName;
use zbus::zvariant::ObjectPath;

use crate::error::{HarnessError, Result};
use crate::types::Capability;

/// Live handle to the accessibility bus.
pub struct AtspiSession {
    conn: AccessibilityConnection,
}

impl AtspiSession {
    /// Connect to the a11y bus. Ensures session `IsEnabled` is true so toolkits expose trees.
    pub async fn connect() -> Result<Self> {
        if let Err(e) = atspi::connection::set_session_accessibility(true).await {
            warn!(error = %e, "could not set session accessibility IsEnabled");
        }

        let conn = AccessibilityConnection::new()
            .await
            .map_err(|e| HarnessError::Unavailable(format!("AT-SPI connection failed: {e}")))?;

        let root = conn
            .root_accessible_on_registry()
            .await
            .map_err(|e| HarnessError::Unavailable(format!("registry root: {e}")))?;
        let _ = root
            .child_count()
            .await
            .map_err(|e| HarnessError::Unavailable(format!("registry child_count: {e}")))?;

        debug!("AT-SPI session ready");
        Ok(Self { conn })
    }

    pub fn connection(&self) -> &zbus::Connection {
        self.conn.connection()
    }

    pub fn inner(&self) -> &AccessibilityConnection {
        &self.conn
    }

    /// Registry desktop root (parent of application accessibles).
    pub async fn registry_root(&self) -> Result<AccessibleProxy<'_>> {
        self.conn
            .root_accessible_on_registry()
            .await
            .map_err(|e| HarnessError::Unavailable(format!("registry root: {e}")))
    }

    /// Resolve `{bus}|{path}` to an AccessibleProxy (lifetime tied to the session).
    pub async fn proxy_for_id(&self, id: &str) -> Result<AccessibleProxy<'_>> {
        let (bus, path) = crate::a11y::id::decode_id(id).ok_or_else(|| {
            HarnessError::Other(format!(
                "invalid element id '{id}' — expected '{{bus}}|{{object_path}}'"
            ))
        })?;

        let name = UniqueName::try_from(bus.to_string())
            .map_err(|e| HarnessError::Other(format!("bad bus name '{bus}': {e}")))?;
        let path = ObjectPath::try_from(path.to_string())
            .map_err(|e| HarnessError::Other(format!("bad object path: {e}")))?;
        let owned = ObjectRef::new_owned(name, path);

        owned
            .into_accessible_proxy(self.connection())
            .await
            .map_err(|e| HarnessError::NotFound(format!("id {id}: {e}")))
    }

    /// Best-effort Unix PID for a unique bus name on the a11y bus.
    pub async fn pid_for_bus(&self, bus: &str) -> Option<u32> {
        let dbus = zbus::fdo::DBusProxy::new(self.connection()).await.ok()?;
        let name = zbus::names::BusName::try_from(bus).ok()?;
        dbus.get_connection_unix_process_id(name).await.ok()
    }
}

/// Probe AT-SPI for `doctor` without failing the whole report.
pub async fn probe_atspi() -> Capability {
    match AtspiSession::connect().await {
        Ok(session) => match session.registry_root().await {
            Ok(root) => match root.get_children().await {
                Ok(kids) => Capability {
                    name: "atspi".into(),
                    available: true,
                    detail: Some(format!("bus live · {} application root(s)", kids.len())),
                },
                Err(e) => Capability {
                    name: "atspi".into(),
                    available: true,
                    detail: Some(format!("connected but get_children failed: {e}")),
                },
            },
            Err(e) => Capability {
                name: "atspi".into(),
                available: false,
                detail: Some(e.to_string()),
            },
        },
        Err(e) => Capability {
            name: "atspi".into(),
            available: false,
            detail: Some(e.to_string()),
        },
    }
}
