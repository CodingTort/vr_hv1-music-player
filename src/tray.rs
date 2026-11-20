//! System tray icon: a StatusNotifierItem (what Waybar's `tray` module
//! shows) with a dbusmenu right-click menu. It runs on its own thread and
//! D-Bus connection; clicks come back to the app as `TrayCommand`s, and the
//! playing song goes the other way for the tooltip.
//!
//! Left click shows the window, middle click plays/pauses, scrolling skips
//! tracks, right click opens the menu.

use std::collections::HashMap;

use iced::futures::StreamExt;

use tokio::sync::mpsc::{UnboundedReceiver, UnboundedSender};
use zbus::object_server::SignalEmitter;
use zbus::zvariant::{OwnedObjectPath, OwnedValue, StructureBuilder, Value};

use crate::audio::mpris::APP_NAME;

/// StatusNotifierItem tooltip: (icon name, icon pixmaps (w, h, ARGB), title, text).
type ToolTip = (String, Vec<(i32, i32, Vec<u8>)>, String, String);
/// dbusmenu layout node: (id, properties, children).
type MenuLayout = (i32, HashMap<String, OwnedValue>, Vec<OwnedValue>);

#[derive(Debug, Clone, Copy)]
pub enum TrayCommand {
    ShowWindow,
    PlayPause,
    Next,
    Previous,
    Quit,
}

const ITEM_PATH: &str = "/StatusNotifierItem";
const MENU_PATH: &str = "/MenuBar";

/// The menu, by dbusmenu item id. "-" is a separator.
const MENU: &[(i32, &str, Option<TrayCommand>)] = &[
    (1, "Show window", Some(TrayCommand::ShowWindow)),
    (2, "Play / Pause", Some(TrayCommand::PlayPause)),
    (3, "Next", Some(TrayCommand::Next)),
    (4, "Previous", Some(TrayCommand::Previous)),
    (5, "-", None),
    (6, "Quit", Some(TrayCommand::Quit)),
];

pub fn launch(commands: UnboundedSender<TrayCommand>, now_playing: UnboundedReceiver<String>) {
    std::thread::spawn(move || {
        let Ok(rt) = tokio::runtime::Builder::new_current_thread().enable_all().build() else { return };
        rt.block_on(async move {
            if let Err(e) = run(commands, now_playing).await {
                eprintln!("tray: {e}");
            }
        });
    });
}

async fn run(commands: UnboundedSender<TrayCommand>, mut now_playing: UnboundedReceiver<String>) -> zbus::Result<()> {
    let name = format!("org.kde.StatusNotifierItem-{}-1", std::process::id());
    let conn = zbus::connection::Builder::session()?
        .name(name.as_str())?
        .serve_at(ITEM_PATH, Item { commands: commands.clone(), song: String::new() })?
        .serve_at(MENU_PATH, Menu { commands })?
        .build()
        .await?;

    // Waybar (or whichever tray is running) shows items registered with its
    // StatusNotifierWatcher. Register now if it's there, and again whenever
    // it comes back: Omarchy restarts Waybar on every theme switch.
    const WATCHER: &str = "org.kde.StatusNotifierWatcher";
    register(&conn, &name).await;
    let bus = zbus::fdo::DBusProxy::new(&conn).await?;
    let mut watcher_changes = bus.receive_name_owner_changed_with_args(&[(0, WATCHER)]).await?;

    loop {
        tokio::select! {
            Some(change) = watcher_changes.next() => {
                if change.args().is_ok_and(|a| a.new_owner().is_some()) {
                    register(&conn, &name).await;
                }
            }
            song = now_playing.recv() => {
                let Some(song) = song else { return Ok(()) };
                let item = conn.object_server().interface::<_, Item>(ITEM_PATH).await?;
                item.get_mut().await.song = song;
                Item::new_tool_tip(item.signal_emitter()).await?;
            }
        }
    }
}

/// Announces the icon to the tray. Failing is normal when no tray runs.
async fn register(conn: &zbus::Connection, name: &str) {
    let registered = async {
        zbus::Proxy::new(conn, "org.kde.StatusNotifierWatcher", "/StatusNotifierWatcher", "org.kde.StatusNotifierWatcher")
            .await?
            .call_method("RegisterStatusNotifierItem", &(name,))
            .await
    };
    if let Err(e) = registered.await {
        eprintln!("tray: no system tray to show the icon in ({e})");
    }
}

struct Item {
    commands: UnboundedSender<TrayCommand>,
    /// "Title — Artist", for the tooltip.
    song: String,
}

#[zbus::interface(name = "org.kde.StatusNotifierItem")]
impl Item {
    #[zbus(property)]
    fn category(&self) -> &str {
        "ApplicationStatus"
    }

    #[zbus(property)]
    fn id(&self) -> &str {
        "lavanda"
    }

    #[zbus(property)]
    fn title(&self) -> &str {
        APP_NAME
    }

    #[zbus(property)]
    fn status(&self) -> &str {
        "Active"
    }

    /// Tux, installed in the user's icon theme with the desktop entry.
    #[zbus(property)]
    fn icon_name(&self) -> &str {
        "tort-music-player"
    }

    #[zbus(property)]
    fn icon_theme_path(&self) -> String {
        format!("{}/.local/share/icons", std::env::var("HOME").unwrap_or_default())
    }

    #[zbus(property)]
    fn item_is_menu(&self) -> bool {
        false
    }

    #[zbus(property)]
    fn menu(&self) -> OwnedObjectPath {
        OwnedObjectPath::try_from(MENU_PATH).expect("valid object path")
    }

    /// (icon name, icon pixmaps, title, text).
    #[zbus(property)]
    fn tool_tip(&self) -> ToolTip {
        (String::new(), Vec::new(), APP_NAME.to_string(), self.song.clone())
    }

    /// Left click.
    fn activate(&self, _x: i32, _y: i32) {
        let _ = self.commands.send(TrayCommand::ShowWindow);
    }

    /// Middle click.
    fn secondary_activate(&self, _x: i32, _y: i32) {
        let _ = self.commands.send(TrayCommand::PlayPause);
    }

    /// Scroll wheel over the icon.
    fn scroll(&self, delta: i32, orientation: &str) {
        if orientation.eq_ignore_ascii_case("vertical") && delta != 0 {
            let _ = self.commands.send(if delta < 0 { TrayCommand::Next } else { TrayCommand::Previous });
        }
    }

    #[zbus(signal)]
    async fn new_tool_tip(emitter: &SignalEmitter<'_>) -> zbus::Result<()>;
}

struct Menu {
    commands: UnboundedSender<TrayCommand>,
}

fn owned(value: Value<'_>) -> zbus::fdo::Result<OwnedValue> {
    OwnedValue::try_from(value).map_err(|e| zbus::fdo::Error::Failed(e.to_string()))
}

fn item_properties(id: i32) -> zbus::fdo::Result<HashMap<String, OwnedValue>> {
    let mut props = HashMap::new();
    if id == 0 {
        props.insert("children-display".into(), owned(Value::from("submenu"))?);
    } else if let Some((_, label, _)) = MENU.iter().find(|(i, ..)| *i == id) {
        if *label == "-" {
            props.insert("type".into(), owned(Value::from("separator"))?);
        } else {
            props.insert("label".into(), owned(Value::from(*label))?);
        }
    }
    Ok(props)
}

/// A menu entry in dbusmenu's layout format: (id, properties, children).
fn layout_item(id: i32) -> zbus::fdo::Result<OwnedValue> {
    let structure = StructureBuilder::new()
        .add_field(id)
        .add_field(item_properties(id)?)
        .add_field(Vec::<OwnedValue>::new())
        .build()
        .map_err(|e| zbus::fdo::Error::Failed(e.to_string()))?;
    owned(Value::from(structure))
}

impl Menu {
    fn clicked(&self, id: i32) {
        if let Some((_, _, Some(command))) = MENU.iter().find(|(i, ..)| *i == id) {
            let _ = self.commands.send(*command);
        }
    }
}

#[zbus::interface(name = "com.canonical.dbusmenu")]
impl Menu {
    #[zbus(property)]
    fn version(&self) -> u32 {
        3
    }

    #[zbus(property)]
    fn text_direction(&self) -> &str {
        "ltr"
    }

    #[zbus(property)]
    fn status(&self) -> &str {
        "normal"
    }

    #[zbus(property)]
    fn icon_theme_path(&self) -> Vec<String> {
        Vec::new()
    }

    fn get_layout(
        &self,
        parent_id: i32,
        _recursion_depth: i32,
        _property_names: Vec<String>,
    ) -> zbus::fdo::Result<(u32, MenuLayout)> {
        let children = if parent_id == 0 {
            MENU.iter().map(|(id, ..)| layout_item(*id)).collect::<zbus::fdo::Result<Vec<_>>>()?
        } else {
            Vec::new()
        };
        Ok((1, (parent_id, item_properties(parent_id)?, children)))
    }

    fn get_group_properties(
        &self,
        ids: Vec<i32>,
        _property_names: Vec<String>,
    ) -> zbus::fdo::Result<Vec<(i32, HashMap<String, OwnedValue>)>> {
        let ids = if ids.is_empty() { MENU.iter().map(|(id, ..)| *id).collect() } else { ids };
        ids.into_iter().map(|id| Ok((id, item_properties(id)?))).collect()
    }

    fn get_property(&self, id: i32, name: String) -> zbus::fdo::Result<OwnedValue> {
        item_properties(id)?
            .remove(&name)
            .ok_or_else(|| zbus::fdo::Error::InvalidArgs(format!("no property {name} on item {id}")))
    }

    fn event(&self, id: i32, event_id: String, _data: OwnedValue, _timestamp: u32) {
        if event_id == "clicked" {
            self.clicked(id);
        }
    }

    fn event_group(&self, events: Vec<(i32, String, OwnedValue, u32)>) -> Vec<i32> {
        for (id, event_id, _, _) in events {
            if event_id == "clicked" {
                self.clicked(id);
            }
        }
        Vec::new()
    }

    fn about_to_show(&self, _id: i32) -> bool {
        false
    }

    fn about_to_show_group(&self, _ids: Vec<i32>) -> (Vec<i32>, Vec<i32>) {
        (Vec::new(), Vec::new())
    }

    #[zbus(signal)]
    async fn layout_updated(emitter: &SignalEmitter<'_>, revision: u32, parent: i32) -> zbus::Result<()>;
}
