from pathlib import Path

file = Path(
    r"E:\Projects\r2h-second-brain\apps\desktop\src-tauri\src\lib.rs"
)

backup = file.with_name(file.name + ".before-close-requested-fix")
if not backup.exists():
    backup.write_bytes(file.read_bytes())

text = file.read_text(encoding="utf-8")

old_import = '''use std::{env, path::PathBuf, thread};'''

new_import = '''use std::{
    env,
    path::PathBuf,
    sync::atomic::{AtomicBool, Ordering},
    thread,
};'''

if old_import not in text:
    raise RuntimeError("std import block was not found")

text = text.replace(old_import, new_import, 1)

marker = '''pub const COMMAND_NAMES: [&str; 21] = ['''

if marker not in text:
    raise RuntimeError("COMMAND_NAMES marker was not found")

text = text.replace(
    marker,
    '''static SHUTDOWN_STARTED: AtomicBool = AtomicBool::new(false);

pub const COMMAND_NAMES: [&str; 21] = [''',
    1,
)

old_builder = '''    let application = tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .setup(|app| {'''

new_builder = '''    let application = tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .on_window_event(|window, event| {
            if !matches!(
                event,
                tauri::WindowEvent::CloseRequested { .. }
            ) {
                return;
            }

            let tauri::WindowEvent::CloseRequested { api, .. } = event
            else {
                return;
            };

            api.prevent_close();

            if SHUTDOWN_STARTED.swap(true, Ordering::SeqCst) {
                return;
            }

            let app_handle = window.app_handle().clone();

            let local_model_runtime = app_handle
                .try_state::<LocalModelRuntimeManager>()
                .map(|runtime| runtime.inner().clone());

            let khoj_runtime = app_handle
                .try_state::<KhojRuntimeManager>()
                .map(|runtime| runtime.inner().clone());

            thread::spawn(move || {
                if let Some(runtime) = local_model_runtime {
                    let _ = runtime.stop();
                }

                if let Some(runtime) = khoj_runtime {
                    let _ = runtime.stop();
                }

                app_handle.exit(0);
            });
        })
        .setup(|app| {'''

if old_builder not in text:
    raise RuntimeError("Tauri builder insertion point was not found")

text = text.replace(old_builder, new_builder, 1)

file.write_text(text, encoding="utf-8", newline="\n")

print("TAURI_CLOSE_REQUESTED_SHUTDOWN_PATCHED")
