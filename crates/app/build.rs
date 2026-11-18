#[cfg(windows)]
fn main() {
    winresource::WindowsResource::new()
        .set_icon("../../assets/pith.ico")
        .set("ProductName", "Pith")
        .set("FileDescription", "Local autocomplete for every text box")
        .compile()
        .expect("embed the icon and version info");
}

#[cfg(not(windows))]
fn main() {}
