//! HexaGraph desktop application.

#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod app;
mod canvas;
mod colors;
mod dialogs;
mod export;
mod graph_view;
mod grid;
mod icons;
#[cfg(target_os = "macos")]
mod macos;
mod paint;
mod results;
mod theme;

fn main() -> eframe::Result<()> {
    // In the background: the window does not wait for the registry.
    #[cfg(windows)]
    std::thread::spawn(register_file_type);
    #[cfg(target_os = "macos")]
    macos::install();
    let mut args = std::env::args().skip(1);
    let mut path = None;
    let mut view = None;
    let mut dark = None;
    while let Some(a) = args.next() {
        if a == "--dark" {
            dark = Some(true);
        } else if a == "--view" {
            view = args.next();
        } else {
            path = Some(std::path::PathBuf::from(a));
        }
    }
    let options = eframe::NativeOptions {
        viewport: eframe::egui::ViewportBuilder::default()
            .with_inner_size([1280.0, 820.0])
            .with_min_inner_size([800.0, 500.0])
            .with_title("HexaGraph")
            .with_icon(app_icon()),
        ..Default::default()
    };
    eframe::run_native(
        "HexaGraph",
        options,
        Box::new(move |cc| {
            #[cfg(target_os = "macos")]
            let _ = macos::CTX.set(cc.egui_ctx.clone());
            let stored_dark = cc.storage.and_then(|s| eframe::get_value(s, "dark"));
            let mut app = app::App::new(&cc.egui_ctx, path, view, dark.or(stored_dark).unwrap_or(false));
            if let Some(palettes) = cc.storage.and_then(|s| eframe::get_value(s, "palettes")) {
                app.user_palettes = palettes;
            }
            if let Some(styles) = cc.storage.and_then(|s| eframe::get_value(s, "styles")) {
                app.user_styles = styles;
            }
            Ok(Box::new(app))
        }),
    )
}

/// Window icon rendered from the SVG logo.
fn app_icon() -> eframe::egui::IconData {
    use resvg::{tiny_skia, usvg};
    let tree = usvg::Tree::from_str(include_str!("../../../assets/logo.svg"), &usvg::Options::default()).expect("valid logo");
    let mut pixmap = tiny_skia::Pixmap::new(256, 256).expect("pixmap");
    resvg::render(&tree, tiny_skia::Transform::identity(), &mut pixmap.as_mut());
    // tiny-skia stores premultiplied colors; the icon wants straight alpha.
    let rgba = pixmap
        .pixels()
        .iter()
        .flat_map(|p| {
            let c = p.demultiply();
            [c.red(), c.green(), c.blue(), c.alpha()]
        })
        .collect();
    eframe::egui::IconData { rgba, width: 256, height: 256 }
}

/// Associates .hxn files with this executable for the current user, with the
/// document icon embedded by build.rs. Runs at every start, so it follows the
/// app when it is moved; no administrator rights are needed.
#[cfg(windows)]
fn register_file_type() {
    use std::os::windows::process::CommandExt;
    let Ok(exe) = std::env::current_exe() else { return };
    let exe = exe.display();
    let keys = [
        (r"HKCU\Software\Classes\.hxn", "HexaGraph.Project".to_string()),
        (r"HKCU\Software\Classes\HexaGraph.Project", "HexaGraph project".to_string()),
        (r"HKCU\Software\Classes\HexaGraph.Project\DefaultIcon", format!("\"{exe}\",-2")),
        (r"HKCU\Software\Classes\HexaGraph.Project\shell\open\command", format!("\"{exe}\" \"%1\"")),
    ];
    for (key, value) in keys {
        // CREATE_NO_WINDOW: no console flashes at startup.
        let _ = std::process::Command::new("reg").args(["add", key, "/ve", "/d", &value, "/f"]).creation_flags(0x0800_0000).status();
    }
    // Tell Explorer to reload its icons, so .hxn files show theirs right away.
    let _ = std::process::Command::new("ie4uinit.exe").arg("-show").creation_flags(0x0800_0000).status();
}
