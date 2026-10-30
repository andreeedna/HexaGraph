fn main() {
    // Windows: icon 1 is the app, icon 2 the .hxn documents (see register_file_type in main.rs).
    #[cfg(windows)]
    winresource::WindowsResource::new()
        .set_icon("../../assets/icon.ico")
        .set_icon_with_id("../../assets/document.ico", "2")
        .compile()
        .expect("Windows resources");
}
