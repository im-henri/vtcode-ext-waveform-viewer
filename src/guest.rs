use crate::{
    loader::{Loader, Source},
    viewer::{Event, Session, Viewer},
};
use std::{
    cell::{Cell, RefCell},
    collections::{HashMap, VecDeque},
};
use vtcode_extension_api::{self as api, editor as host};

struct OpenViewer {
    viewer: Viewer,
    loader: Option<Loader>,
    needs_render: bool,
}
thread_local! { static VIEWS: RefCell<HashMap<u32, OpenViewer>> = RefCell::new(HashMap::new()); }
struct PendingInput {
    json: String,
    clipboard: String,
    motion: Option<PointerMotion>,
}
#[derive(PartialEq, Eq)]
enum PointerMotion {
    Hover(String),
    Drag(String, [bool; 4]),
    Resize(String),
}
thread_local! { static INPUTS: RefCell<VecDeque<PendingInput>> = const { RefCell::new(VecDeque::new()) }; }
thread_local! { static LAST_RENDERED: Cell<u32> = const { Cell::new(0) }; }
thread_local! { static RENDERER: RefCell<crate::render::Renderer> = RefCell::new(crate::render::Renderer::default()); }
struct HostSource(u32);
impl Source for HostSource {
    fn read(&mut self, offset: u64, count: usize) -> Result<Vec<u8>, String> {
        host::read_document(self.0, offset, count)
    }
    fn read_into(&mut self, offset: u64, out: &mut [u8]) -> Result<usize, String> {
        host::read_document_into(self.0, offset, out)
    }
}
fn view_id(id: u32) -> String {
    format!("wave:{id}")
}
fn session_key(path: &str) -> String {
    // Stable across processes/platforms; no randomized HashMap hasher in filenames.
    let hash = path.bytes().fold(0xcbf29ce484222325u64, |hash, byte| {
        (hash ^ u64::from(byte)).wrapping_mul(0x100000001b3)
    });
    format!("session:{hash:016x}")
}
fn update_tasks(views: &HashMap<u32, OpenViewer>) {
    host::register_task_handler(
        "wave_apply",
        if INPUTS.with(|inputs| !inputs.borrow().is_empty()) {
            16
        } else {
            0
        },
    );
    host::register_task_handler(
        "wave_tick",
        if views.values().any(|v| {
            v.loader.is_some()
                || v.viewer.search.is_some()
                || v.viewer.edge_search.is_some()
                || v.viewer.needs_drag_tick()
        }) {
            16
        } else {
            0
        },
    );
    host::register_task_handler(
        "wave_render",
        if views.values().any(|v| v.needs_render) {
            16
        } else {
            0
        },
    );
}
fn save(viewer: &mut Viewer) {
    if !std::mem::take(&mut viewer.save_requested) {
        return;
    }
    match serde_json::to_string(&viewer.session) {
        Ok(json) if api::save_setting(&session_key(&viewer.path), &json) == 0 => {
            viewer.status =
                "Waveform session saved · signal list, cursors, markers and zoom".into();
        }
        _ => viewer.fail("Could not save waveform session"),
    }
}
fn save_on_close(viewer: &mut Viewer) {
    if !viewer.wave.signals.is_empty() {
        viewer.save_requested = true;
        save(viewer);
    }
}
fn reload(open: &mut OpenViewer, id: u32) {
    let result = (|| -> Result<(), String> {
        host::document_action(id, host::action::RELOAD)?;
        let info = host::document_info(id)?;
        let session = if open.viewer.wave.signals.is_empty() {
            open.viewer.pending_session.take()
        } else {
            Some(open.viewer.session.clone())
        };
        let (width, height) = (open.viewer.width, open.viewer.height);
        let mut viewer = Viewer::new(info.path.clone(), Some(id));
        viewer.width = width;
        viewer.height = height;
        viewer.pending_session = session;
        let loader = Loader::new(info.size, info.path.to_ascii_lowercase().ends_with(".fst"));
        viewer.loading = Some((0, info.size));
        open.viewer = viewer;
        open.loader = Some(loader?);
        Ok(())
    })();
    if let Err(error) = result {
        open.viewer.fail(error);
        open.viewer.loading = None;
        open.loader = None;
    }
    open.needs_render = true;
}
fn ensure_open(views: &mut HashMap<u32, OpenViewer>, id: u32) -> Result<&mut OpenViewer, String> {
    if let std::collections::hash_map::Entry::Vacant(entry) = views.entry(id) {
        let info = host::document_info(id)?;
        let mut viewer = Viewer::new(info.path.clone(), Some(id));
        viewer.pending_session = api::load_setting(&session_key(&info.path))
            .and_then(|json| serde_json::from_str::<Session>(&json).ok());
        let open = entry.insert(OpenViewer {
            viewer,
            loader: None,
            needs_render: true,
        });
        reload(open, id);
    }
    Ok(views.get_mut(&id).unwrap())
}

#[unsafe(no_mangle)]
pub extern "C" fn vtcode_activate() -> i32 {
    api::register_command("wave.open", "Open File as Waveform");
    api::register_explorer_context_command("wave.open", "Open With Waveform Viewer");
    api::register_file_open_command("wave.open", "Waveform Viewer", "vcd,fst");
    api::set_status("VCD / FST waveform explorer");
    0
}
#[unsafe(no_mangle)]
pub extern "C" fn vtcode_deactivate() -> i32 {
    VIEWS.with(|views| {
        for (_, mut open) in views.borrow_mut().drain() {
            save_on_close(&mut open.viewer);
        }
    });
    0
}
#[unsafe(no_mangle)]
pub extern "C" fn vtcode_run_command(index: i32) -> i32 {
    if index != 0 {
        return 1;
    }
    let result = (|| -> Result<(), String> {
        let id = host::open_document()?;
        let view = view_id(id);
        api::register_view(&view, "Waveform Viewer", "");
        api::register_view_key_handler(&view, "wave_event");
        api::register_view_mouse_handler(&view, "wave_event");
        api::register_view_scroll_handler(&view, "wave_event");
        host::register_resize_handler(&view, "wave_event");
        host::register_close_handler(&view, "wave_event");
        host::register_hover_handler(&view, "wave_event");
        host::register_context_menu_handler(&view, "wave_event");
        host::enable_pointer_capture(&view, true);
        let info = host::document_info(id)?;
        let mut placeholder = Viewer::new(info.path, Some(id));
        placeholder.loading = Some((0, info.size));
        host::set_frame(&view, &placeholder.render())?;
        // Opening is ordered with earlier close/input events, so a rapid reopen
        // cannot be removed by a close whose cleanup worker has not run yet.
        INPUTS.with(|inputs| {
            inputs.borrow_mut().push_back(PendingInput {
                json: serde_json::json!({ "viewId": view, "code": "open" }).to_string(),
                clipboard: String::new(),
                motion: None,
            })
        });
        host::register_task_handler("wave_apply", 16);
        api::open_view(&view);
        Ok(())
    })();
    if let Err(error) = result {
        api::set_status(&error);
        api::log(&error);
        return 1;
    }
    0
}

#[unsafe(no_mangle)]
pub extern "C" fn wave_event() -> i32 {
    // Capture input and its clipboard snapshot in the short UI callback. Filtering
    // a large hierarchy, importing a session, or freeing a closed dump belongs
    // to the independently budgeted background worker.
    let Some(json) = api::current_view_event() else {
        return 2;
    };
    // Large pasted sessions stay opaque in the short UI callback. Consecutive
    // pointer updates coalesce; a release or key is always an ordering barrier.
    let motion = (json.len() < 2048)
        .then(|| serde_json::from_str::<serde_json::Value>(&json).ok())
        .flatten()
        .and_then(|event| {
            let id = event["viewId"].as_str()?.to_owned();
            match event["kind"].as_str()? {
                "resize" => Some(PointerMotion::Resize(id)),
                "move" | "leave" => Some(PointerMotion::Hover(id)),
                "drag" => Some(PointerMotion::Drag(
                    id,
                    [
                        event["shift"] == true,
                        event["control"] == true,
                        event["alt"] == true,
                        event["pointerInside"] != false,
                    ],
                )),
                _ => None,
            }
        });
    // Loaded-view resize is small and needs no session/file work. Apply it in
    // the capture callback when earlier input has already drained, avoiding an
    // extra 16 ms apply turn per pane. Keep queued keys/gestures ordered.
    if let Some(PointerMotion::Resize(view)) = &motion
        && INPUTS.with(|inputs| inputs.borrow().is_empty())
        && let Some(id) = view
            .strip_prefix("wave:")
            .and_then(|id| id.parse::<u32>().ok())
        && let Ok(event) = serde_json::from_str::<Event>(&json)
    {
        let applied = VIEWS.with(|views| {
            let mut views = views.borrow_mut();
            let Some(open) = views.get_mut(&id) else {
                return false;
            };
            open.viewer.event(event);
            open.needs_render = true;
            update_tasks(&views);
            true
        });
        if applied {
            return 1;
        }
    }
    let clipboard = host::clipboard().unwrap_or_default();
    let accepted = INPUTS.with(|inputs| {
        let mut inputs = inputs.borrow_mut();
        if let Some(PointerMotion::Resize(id)) = &motion {
            // Two split panes alternate resize events. Coalesce each pane in
            // the trailing resize burst without crossing a key or gesture.
            for input in inputs.iter_mut().rev() {
                let Some(PointerMotion::Resize(old)) = &input.motion else {
                    break;
                };
                if old == id {
                    input.json = json;
                    return true;
                }
            }
        }
        if motion.is_some() && inputs.back().is_some_and(|input| input.motion == motion) {
            inputs.pop_back();
        }
        let bytes: usize = inputs
            .iter()
            .map(|input| input.json.len() + input.clipboard.len())
            .sum();
        if inputs.len() >= 256 || bytes + json.len() + clipboard.len() > 8 * 1024 * 1024 {
            return false;
        }
        inputs.push_back(PendingInput {
            json,
            clipboard,
            motion,
        });
        true
    });
    if !accepted {
        api::set_status("Waveform input queue is full; wait for the current operation");
        return 2;
    }
    host::register_task_handler("wave_apply", 16);
    1
}

#[unsafe(no_mangle)]
pub extern "C" fn wave_apply() -> i32 {
    let result = (|| -> Result<(), String> {
        let Some(PendingInput {
            json, clipboard, ..
        }) = INPUTS.with(|inputs| inputs.borrow_mut().pop_front())
        else {
            return Ok(());
        };
        let value: serde_json::Value = serde_json::from_str(&json).map_err(|e| e.to_string())?;
        let event_view_id = value["viewId"].as_str().ok_or("Missing waveform view ID")?;
        let id = event_view_id
            .strip_prefix("wave:")
            .ok_or("Invalid waveform view ID")?
            .parse::<u32>()
            .map_err(|e| e.to_string())?;
        let mut event: Event = serde_json::from_str(&json).map_err(|e| e.to_string())?;
        VIEWS.with(|views| -> Result<(), String> {
            let mut views = views.borrow_mut();
            if event.code == "closed" {
                if let Some(mut open) = views.remove(&id) {
                    save_on_close(&mut open.viewer);
                }
                if views.is_empty() {
                    RENDERER.with(|renderer| {
                        *renderer.borrow_mut() = crate::render::Renderer::default()
                    });
                    LAST_RENDERED.set(0);
                }
                // Retain a small document association, not the old trace frame.
                // A palette-open of this registered view now requests a resize,
                // which reinitializes it from disk even at the same terminal size.
                host::set_frame(
                    &view_id(id),
                    &host::Frame {
                        document: Some(id),
                        ..host::Frame::default()
                    },
                )?;
                update_tasks(&views);
                return Ok(());
            }
            if event.code == "open" {
                ensure_open(&mut views, id)?.needs_render = true;
                update_tasks(&views);
                return Ok(());
            }
            let open = ensure_open(&mut views, id)?;
            if event.code == "command" && event.text == "paste"
                || event.control && event.text.eq_ignore_ascii_case("v")
            {
                if open.viewer.prompt.is_none() {
                    open.viewer.open_prompt(crate::viewer::Prompt::Import);
                }
                event.code = "paste".into();
                event.text = clipboard;
                event.control = false;
            }
            if event.code == "paste" && open.viewer.prompt.is_none() {
                open.viewer.open_prompt(crate::viewer::Prompt::Import);
            }
            open.viewer.event(event);
            if std::mem::take(&mut open.viewer.reload_requested) {
                reload(open, id);
            }
            if open.viewer.cancelled {
                open.loader = None;
            }
            save(&mut open.viewer);
            if let Some(text) = open.viewer.copy_requested.take() {
                host::set_clipboard(&text)?;
            }
            // The input callback stays small even in a 512×200 terminal. The
            // background renderer coalesces changes and publishes the next frame.
            open.needs_render = true;
            update_tasks(&views);
            Ok(())
        })
    })();
    VIEWS.with(|views| update_tasks(&views.borrow()));
    if let Err(error) = result {
        api::set_status(&error);
        api::log(&error);
        return 2;
    }
    0
}

#[unsafe(no_mangle)]
pub extern "C" fn wave_tick() -> i32 {
    VIEWS.with(|views| {
        let mut views = views.borrow_mut();
        let total_bytes: usize = views
            .values()
            .map(|v| {
                v.viewer.wave.data_bytes + v.loader.as_ref().map(|l| l.wave.data_bytes).unwrap_or(0)
            })
            .sum();
        let ids: Vec<u32> = views.keys().copied().collect();
        for id in ids {
            let open = views.get_mut(&id).unwrap();
            if let Some(loader) = &open.loader {
                let other_bytes = total_bytes.saturating_sub(loader.wave.data_bytes);
                let before = loader.offset;
                // Adapt to actual work: stop at the host's short time slice,
                // keeping a 1 MiB ceiling even on extremely cheap input.
                for _ in 0..64 {
                    let Some(loader) = &mut open.loader else {
                        break;
                    };
                    loader.wave.data_limit = (56 * 1024 * 1024usize).saturating_sub(other_bytes);
                    match loader.step(&mut HostSource(id)) {
                        Ok(Some(wave)) => {
                            open.viewer.ready(wave);
                            open.loader = None;
                        }
                        Ok(None) => {
                            open.viewer.loading = Some((loader.offset, loader.size));
                        }
                        Err(error) => {
                            open.viewer.fail(error);
                            open.viewer.loading = None;
                            open.loader = None;
                        }
                    }
                    if open
                        .loader
                        .as_ref()
                        .is_none_or(|loader| loader.needs_decode_pass())
                        || host::task_should_yield()
                    {
                        break;
                    }
                }
                // Progress is visible without rebuilding a complete terminal frame
                // for each 16 KiB chunk of a large file.
                let show_progress = open.loader.as_ref().is_none_or(|l| {
                    l.offset / (256 * 1024) != before / (256 * 1024) || l.offset == l.size
                });
                if show_progress {
                    open.needs_render = true;
                }
                break;
            }
            if open.viewer.search.is_some() || open.viewer.edge_search.is_some() {
                open.viewer.search_step();
                open.needs_render = true;
                break;
            }
            if open.viewer.drag_tick() {
                open.needs_render = true;
                break;
            }
        }
        update_tasks(&views);
    });
    0
}

#[unsafe(no_mangle)]
pub extern "C" fn wave_render() -> i32 {
    VIEWS.with(|views| {
        let mut views = views.borrow_mut();
        // Continue after the previous pane instead of repeatedly choosing the
        // first HashMap entry during a continuous split resize.
        for _ in 0..views.len() {
            let last = LAST_RENDERED.get();
            let Some(id) = views
                .iter()
                .filter(|(_, open)| open.needs_render)
                .map(|(&id, _)| id)
                .min_by_key(|&id| (id <= last, id))
            else {
                break;
            };
            let open = views.get_mut(&id).unwrap();
            open.needs_render = false;
            LAST_RENDERED.set(id);
            let result = RENDERER.with(|renderer| {
                host::set_frame(&view_id(id), renderer.borrow_mut().render(&open.viewer))
            });
            if let Err(error) = result {
                open.viewer.fail(&error);
                api::set_status(&error);
                api::log(&error);
            }
            if host::task_should_yield() {
                break;
            }
        }
        update_tasks(&views);
    });
    0
}
